#include "image.hpp"
#include <onnxruntime_cxx_api.h>
#include <array>
#include <chrono>
#include <iostream>
#include <numeric>
#include <optional>
#include <regex>
#include <sstream>
#include <string>

using Clock = std::chrono::steady_clock;
struct Text { std::string value; float confidence = 0; };
std::string quote(const std::string& text) {
    std::string out = "\"";
    constexpr char hex[] = "0123456789abcdef";
    for (unsigned char c : text) {
        if (c == '"' || c == '\\') { out += '\\'; out += char(c); }
        else if (c < 32) { out += "\\u00"; out += hex[c >> 4]; out += hex[c & 15]; }
        else out += char(c);
    }
    return out + '"';
}
std::wstring wide(const std::string& text) {
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text.data(), int(text.size()), nullptr, 0);
    std::wstring result(size, 0);
    if (size) MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text.data(), int(text.size()), result.data(), size);
    return result;
}
bool cjk(const std::string& text) {
    for (auto c : wide(text)) if ((c >= 0x3040 && c <= 0x30ff) || (c >= 0x3400 && c <= 0x9fff)) return true;
    return false;
}
bool cyrillic(const std::string& text) {
    for (auto c : wide(text)) if (c >= 0x400 && c <= 0x52f) return true;
    return false;
}
std::optional<int> level(const Text& text) {
    if (text.confidence < 0.90f) return {};
    std::string compact;
    for (unsigned char c : text.value) if (c != ' ') compact += char(c);
    static const std::regex pattern("^([0-9]{1,3})$", std::regex::icase);
    std::smatch match;
    if (!std::regex_match(compact, match, pattern)) return {};
    int value = std::stoi(match[1].str());
    return value <= 100 ? std::optional<int>(value) : std::nullopt;
}
// CTC greedy decoding, same normalization and blank/repeat handling as RapidOCR.
Text decode(const float* values, size_t steps, size_t classes, const std::vector<std::string>& dict) {
    if (classes != dict.size()) throw std::runtime_error("model_dictionary_mismatch");
    Text result; size_t previous = classes, count = 0;
    for (size_t step = 0; step < steps; ++step) {
        auto row = values + step * classes;
        size_t index = size_t(std::max_element(row, row + classes) - row);
        if (index && index != previous) { result.value += dict[index]; result.confidence += row[index]; ++count; }
        previous = index;
    }
    if (count) result.confidence /= float(count);
    auto first = result.value.find_first_not_of(' ');
    result.value = first == std::string::npos ? "" : result.value.substr(first, result.value.find_last_not_of(' ') - first + 1);
    return result;
}
class Recognizer {
    Ort::Session session{nullptr};
    std::vector<std::string> dict;
    std::string input, output;
public:
    Recognizer(Ort::Env& env, const std::filesystem::path& path) {
        Ort::SessionOptions options;
        options.SetIntraOpNumThreads(2); options.SetInterOpNumThreads(1);
        options.SetGraphOptimizationLevel(GraphOptimizationLevel::ORT_ENABLE_ALL);
        session = Ort::Session(env, path.c_str(), options);
        Ort::AllocatorWithDefaultOptions alloc;
        auto characters = session.GetModelMetadata().LookupCustomMetadataMapAllocated("character", alloc);
        if (!characters) throw std::runtime_error("model_dictionary_missing");
        std::istringstream lines(characters.get()); std::string line;
        dict.emplace_back("");
        while (std::getline(lines, line)) { if (!line.empty() && line.back() == '\r') line.pop_back(); dict.push_back(line); }
        dict.emplace_back(" ");
        input = session.GetInputNameAllocated(0, alloc).get();
        output = session.GetOutputNameAllocated(0, alloc).get();
    }
    std::vector<Text> run(const std::vector<Image>& images) {
        if (images.empty()) return {};
        int width = 320;
        for (const auto& image : images) {
            if (image.empty()) throw std::runtime_error("invalid_crop");
            width = std::max(width, int(std::ceil(48.0 * image.width / image.height)));
        }
        if (width > 2048 || images.size() > 20) throw std::runtime_error("crop_limit");
        std::vector<float> data; data.reserve(images.size() * 3 * 48 * width);
        for (const auto& image : images) {
            auto item = tensor(image, width, int(std::ceil(48.0 * image.width / image.height)));
            data.insert(data.end(), item.begin(), item.end());
        }
        std::array<int64_t, 4> shape{int64_t(images.size()), 3, 48, width};
        auto memory = Ort::MemoryInfo::CreateCpu(OrtArenaAllocator, OrtMemTypeDefault);
        auto value = Ort::Value::CreateTensor<float>(memory, data.data(), data.size(), shape.data(), shape.size());
        const char* ins[] = { input.c_str() }; const char* outs[] = { output.c_str() };
        auto predictions = session.Run(Ort::RunOptions{nullptr}, ins, &value, 1, outs, 1);
        auto dimensions = predictions[0].GetTensorTypeAndShapeInfo().GetShape();
        if (dimensions.size() != 3 || dimensions[0] != int64_t(images.size()) || dimensions[1] <= 0 || dimensions[2] != int64_t(dict.size())) throw std::runtime_error("model_shape_mismatch");
        size_t steps = size_t(dimensions[1]), classes = size_t(dimensions[2]);
        const float* results = predictions[0].GetTensorData<float>();
        std::vector<Text> decoded;
        for (size_t i = 0; i < images.size(); ++i) decoded.push_back(decode(results + i * steps * classes, steps, classes, dict));
        return decoded;
    }
};
struct Player { int side, row; Text name; int level; std::string alternative; int friendly; };
bool supported(const Image& image) {
    double aspect = double(image.width) / std::max(1, image.height);
    return image.width >= 1280 && image.height >= 720 && aspect >= 1.74 && aspect <= 1.84;
}
Region name_region(int side, int row) { return {side ? 0.7870 : 0.1201, 0.2917 + row * 0.14255, 0.0920, 0.0188}; }
Region level_region(int side, int row) { return {side ? 0.8039 : 0.1375, 0.3121 + row * 0.14255, 0.0225, 0.0140}; }
// Game card layout scales with viewport width and is vertically centred.
// The reference capture is 3576x1987, not a stretched 16:9 canvas.
Region card_region(const Image& image, Region r) {
    double scale = (1987.0 / 3576.0) * image.width / image.height;
    r.y = 0.5 + (r.y - 0.5) * scale; r.height *= scale;
    return r;
}
int friendly_side(const Image& image, int side) {
    auto header = crop(image, card_region(image, {side ? 0.72 : 0.055, 0.205, 0.20, 0.055}));
    size_t red = 0, blue = 0;
    for (size_t i = 0; i + 2 < header.pixels.size(); i += 3) {
        int b=header.pixels[i], g=header.pixels[i+1], r=header.pixels[i+2];
        red += r > 120 && r > g * 1.5 && r > b * 1.5;
        blue += b > 140 && g > 100 && b > r * 1.4 && g > r * 1.3;
    }
    size_t minimum = std::max(size_t(30), header.pixels.size()/3/100);
    if (blue > minimum && blue > red * 3) return 1;
    if (red > minimum && red > blue * 3) return 0;
    return -1;
}
std::vector<Player> extract(Recognizer& general, Recognizer& russian, const Image& image, bool diagnostic = false) {
    if (!supported(image)) return {};
    std::vector<Image> names, levels;
    for (int side = 0; side < 2; ++side) for (int row = 0; row < 5; ++row) {
        names.push_back(text_crop(crop(image, card_region(image, name_region(side, row))), false));
        levels.push_back(digits_crop(crop(image, card_region(image, level_region(side, row)))));
    }
    auto name_text = general.run(names);
    auto level_text = general.run(levels);
    std::vector<Image> retry_names, retry_levels; std::vector<size_t> name_ids, level_ids;
    for (size_t i = 0; i < names.size(); ++i) {
        if (name_text[i].confidence < 0.95f) { retry_names.push_back(text_crop(crop(image, card_region(image, name_region(int(i/5), int(i%5)))), false, true)); name_ids.push_back(i); }
        if (!level(level_text[i])) { retry_levels.push_back(digits_crop(crop(image, card_region(image, level_region(int(i/5), int(i%5)))), true)); level_ids.push_back(i); }
    }
    auto retried_names = general.run(retry_names);
    for (size_t n = 0; n < name_ids.size(); ++n) if (retried_names[n].confidence > name_text[name_ids[n]].confidence) name_text[name_ids[n]] = retried_names[n];
    auto retried_levels = russian.run(retry_levels);
    for (size_t n = 0; n < level_ids.size(); ++n) if (level(retried_levels[n])) level_text[level_ids[n]] = retried_levels[n];
    std::vector<Image> alternates; std::vector<size_t> indices;
    for (size_t i = 0; i < names.size(); ++i) if (!cjk(name_text[i].value) && level(level_text[i])) { alternates.push_back(names[i]); indices.push_back(i); }
    auto alt_text = russian.run(alternates);
    std::array<std::string, 10> alt{};
    for (size_t n = 0; n < indices.size(); ++n) if (alt_text[n].confidence >= 0.8f && cyrillic(alt_text[n].value) && alt_text[n].value != name_text[indices[n]].value) {
        if (name_text[indices[n]].confidence < 0.85f || (alt_text[n].confidence > 0.95f && wide(alt_text[n].value).size() > 2 * wide(name_text[indices[n]].value).size())) name_text[indices[n]] = alt_text[n];
        else alt[indices[n]] = alt_text[n].value;
    }
    std::array<int,2> friendly{friendly_side(image,0), friendly_side(image,1)};
    if (friendly[0] == friendly[1]) friendly = {-1,-1};
    std::vector<Player> players;
    for (size_t i = 0; i < names.size(); ++i) {
        auto rank = level(level_text[i]);
        if (diagnostic) std::cerr << i << " " << quote(name_text[i].value) << " " << name_text[i].confidence << " " << quote(level_text[i].value) << " " << level_text[i].confidence << '\n';
        if (rank && name_text[i].confidence >= 0.85f && !name_text[i].value.empty() && wide(name_text[i].value).size() <= 80)
            players.push_back({int(i / 5), int(i % 5), name_text[i], *rank, alt[i], friendly[i/5]});
    }
    bool left = false, right = false;
    for (const auto& p : players) { left |= p.side == 0; right |= p.side == 1; }
    if (!left || !right) return {};
    return players;
}
void emit(const std::vector<Player>& players, const std::string& status, double ms) {
    std::cout << "{\"players\":[";
    bool first = true;
    for (const auto& p : players) {
        if (!first) std::cout << ','; first = false;
        std::cout << "{\"side\":" << quote(p.side ? "right" : "left") << ",\"row\":" << p.row << ",\"name\":" << quote(p.name.value) << ",\"level\":" << p.level << ",\"alternatives\":[";
        if (!p.alternative.empty()) std::cout << quote(p.alternative);
        std::cout << "],\"friendly\":" << (p.friendly < 0 ? "null" : (p.friendly ? "true" : "false")) << "}";
    }
    std::cout << "],\"status\":" << quote(status) << ",\"elapsedMs\":" << ms << "}" << std::endl;
}
void require(bool condition) { if (!condition) throw std::runtime_error("unit_test_failed"); }
void unit_test() {
    require(level({"93", .99f}) == 93); require(level({"100", .99f}) == 100);
    require(!level({"101", .99f})); require(!level({"Elo 93", .99f})); require(!level({"93", .5f}));
    require(quote("a\"\n\\") == "\"a\\\"\\u000a\\\\\"");
    std::vector<std::string> dict{"", "A", "B"};
    const float prediction[] = {0,1,0, 0,1,0, 1,0,0, 0,1,0, 0,0,1};
    require(decode(prediction, 5, 3, dict).value == "AAB");
    require(cjk("原子テスト") && cyrillic("Борис") && !cyrillic("Boris"));
    Image image(2, 2); std::fill(image.pixels.begin(), image.pixels.end(), 255);
    auto t = tensor(image, 4, 2); require(t[0] == 1.f && t[2] == 0.f);
    require(crop(image, {1, 1, 1, 1}).empty());
    Image hd; hd.width = 1920; hd.height = 1080; require(supported(hd));
    hd.width = 2560; require(!supported(hd));
    for (int side = 0; side < 2; ++side) for (int row = 0; row < 5; ++row) {
        auto r = name_region(side, row); require(r.x >= 0 && r.y >= 0 && r.x + r.width <= 1 && r.y + r.height <= 1);
    }
    std::cout << "native core tests passed" << std::endl;
}
int wmain(int argc, wchar_t** argv) {
    if (argc == 2 && std::wstring(argv[1]) == L"--unit-test") { unit_test(); return 0; }
    if (argc < 2) return 2;
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    HRESULT initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    try {
        auto start = Clock::now();
        Ort::Env env(ORT_LOGGING_LEVEL_ERROR, "bax-ocr"); env.DisableTelemetryEvents();
        std::filesystem::path models(argv[1]);
        Recognizer general(env, models / "PP-OCRv6_rec_small.onnx"), russian(env, models / "eslav_PP-OCRv5_rec_mobile.onnx");
        general.run({Image(160, 48)}); russian.run({Image(160, 48)});
        double init_ms = std::chrono::duration<double, std::milli>(Clock::now() - start).count();
        std::cout << "{\"ready\":true,\"initializationMs\":" << init_ms << "}" << std::endl;
        if (argc >= 3 && std::wstring(argv[2]) == L"--self-test") { if (SUCCEEDED(initialized)) CoUninitialize(); return 0; }
        if (argc >= 4 && std::wstring(argv[2]) == L"--image") {
            Image image = load_image(argv[3]);
            int repeats = argc >= 5 ? std::clamp(_wtoi(argv[4]), 1, 100) : 1;
            bool diagnostic = argc >= 6 && std::wstring(argv[5]) == L"--diagnostic";
            for (int i = 0; i < repeats; ++i) { start = Clock::now(); auto players = extract(general, russian, image, diagnostic); emit(players, "离线图片测试", std::chrono::duration<double, std::milli>(Clock::now() - start).count()); }
        } else {
            for (std::string command; std::getline(std::cin, command);) {
                if (command.size() > 65536) break;
                start = Clock::now();
                try {
                    auto image = capture_game();
                    auto players = extract(general, russian, image);
                    auto status = image.empty() ? "等待游戏前台画面" : (!supported(image) ? "暂不支持此分辨率或比例" : (players.empty() ? "等待固定双侧玩家卡片" : "已识别卡片"));
                    emit(players, status, std::chrono::duration<double, std::milli>(Clock::now() - start).count());
                } catch (const std::exception&) { emit({}, "画面识别失败，将重试", 0); }
            }
        }
    } catch (const std::exception&) {
        std::cout << "{\"ready\":false,\"error\":\"native_ocr_failed\"}" << std::endl;
        if (SUCCEEDED(initialized)) CoUninitialize(); return 1;
    }
    if (SUCCEEDED(initialized)) CoUninitialize();
    return 0;
}
