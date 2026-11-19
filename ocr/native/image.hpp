#pragma once
#include <windows.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <filesystem>
#include <stdexcept>
#include <vector>

struct Image {
    int width = 0, height = 0;
    std::vector<uint8_t> pixels; // BGR, top down
    Image() = default;
    Image(int w, int h) : width(w), height(h), pixels(size_t(w) * h * 3) {}
    bool empty() const { return pixels.empty(); }
};
struct Region { double x, y, width, height; };
inline Image crop(const Image& image, Region r) {
    int x = std::clamp(int(std::round(r.x * image.width)), 0, image.width);
    int y = std::clamp(int(std::round(r.y * image.height)), 0, image.height);
    int w = std::min(int(std::round(r.width * image.width)), image.width - x);
    int h = std::min(int(std::round(r.height * image.height)), image.height - y);
    if (w <= 0 || h <= 0) return {};
    Image out(w, h);
    for (int row = 0; row < h; ++row)
        std::copy_n(image.pixels.data() + (size_t(y + row) * image.width + x) * 3,
                    size_t(w) * 3, out.pixels.data() + size_t(row) * w * 3);
    return out;
}
inline void check_hr(HRESULT hr) { if (FAILED(hr)) throw std::runtime_error("image_io_failed"); }
inline Image load_image(const std::filesystem::path& path) {
    using Microsoft::WRL::ComPtr;
    ComPtr<IWICImagingFactory> factory;
    check_hr(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)));
    ComPtr<IWICBitmapDecoder> decoder;
    check_hr(factory->CreateDecoderFromFilename(path.c_str(), nullptr, GENERIC_READ, WICDecodeMetadataCacheOnDemand, &decoder));
    ComPtr<IWICBitmapFrameDecode> frame;
    check_hr(decoder->GetFrame(0, &frame));
    UINT w = 0, h = 0;
    check_hr(frame->GetSize(&w, &h));
    if (!w || !h || w > 8192 || h > 8192) throw std::runtime_error("image_size_unsupported");
    ComPtr<IWICFormatConverter> converter;
    check_hr(factory->CreateFormatConverter(&converter));
    check_hr(converter->Initialize(frame.Get(), GUID_WICPixelFormat24bppBGR, WICBitmapDitherTypeNone, nullptr, 0, WICBitmapPaletteTypeCustom));
    Image image(static_cast<int>(w), static_cast<int>(h));
    check_hr(converter->CopyPixels(nullptr, w * 3, static_cast<UINT>(image.pixels.size()), image.pixels.data()));
    return image;
}
inline Image capture_game() {
    HWND hwnd = GetForegroundWindow();
    if (!hwnd || IsIconic(hwnd)) return {};
    DWORD pid = 0; GetWindowThreadProcessId(hwnd, &pid);
    HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    if (!process) return {};
    wchar_t path[32768]; DWORD length = 32768;
    bool got = QueryFullProcessImageNameW(process, 0, path, &length) != 0;
    CloseHandle(process);
    if (!got) return {};
    auto name = std::filesystem::path(path).filename().wstring();
    if (_wcsicmp(name.c_str(), L"BrokenArrow.exe") && _wcsicmp(name.c_str(), L"Broken_Arrow.exe") && _wcsicmp(name.c_str(), L"Broken Arrow.exe")) return {};
    RECT rect{}; POINT point{};
    if (!GetClientRect(hwnd, &rect) || !ClientToScreen(hwnd, &point)) return {};
    int w = rect.right, h = rect.bottom;
    if (w < 960 || h < 540 || w > 8192 || h > 8192) return {};
    HDC screen = GetDC(nullptr);
    HDC memory = screen ? CreateCompatibleDC(screen) : nullptr;
    if (!memory) { if (screen) ReleaseDC(nullptr, screen); return {}; }
    BITMAPINFO info{}; info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    info.bmiHeader.biWidth = w; info.bmiHeader.biHeight = -h;
    info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32; info.bmiHeader.biCompression = BI_RGB;
    void* bits = nullptr;
    HBITMAP bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &bits, nullptr, 0);
    Image out;
    if (bitmap) {
        auto previous = SelectObject(memory, bitmap);
        bool copied = GetForegroundWindow() == hwnd && BitBlt(memory, 0, 0, w, h, screen, point.x, point.y, SRCCOPY | CAPTUREBLT);
        GdiFlush();
        if (copied && GetForegroundWindow() == hwnd) {
            out = Image(w, h);
            auto source = static_cast<const uint8_t*>(bits);
            for (size_t i = 0; i < size_t(w) * h; ++i) std::copy_n(source + i * 4, 3, out.pixels.data() + i * 3);
        }
        SelectObject(memory, previous); DeleteObject(bitmap);
    }
    DeleteDC(memory); ReleaseDC(nullptr, screen);
    return out;
}
// Bilinear resize with half-pixel coordinates; BGR CHW normalized to [-1, 1].
inline std::vector<float> tensor(const Image& image, int width, int resized_width, int height = 48) {
    if (image.empty() || resized_width > width || resized_width <= 0) throw std::runtime_error("invalid_crop");
    std::vector<float> out(size_t(3) * width * height, 0.f);
    for (int y = 0; y < height; ++y) {
        double sy = std::clamp((y + 0.5) * image.height / height - 0.5, 0.0, double(image.height - 1));
        int y0 = int(sy), y1 = std::min(y0 + 1, image.height - 1); double fy = sy - y0;
        for (int x = 0; x < resized_width; ++x) {
            double sx = std::clamp((x + 0.5) * image.width / resized_width - 0.5, 0.0, double(image.width - 1));
            int x0 = int(sx), x1 = std::min(x0 + 1, image.width - 1); double fx = sx - x0;
            for (int c = 0; c < 3; ++c) {
                auto at = [&](int a, int b) { return image.pixels[(size_t(b) * image.width + a) * 3 + c]; };
                double value = (at(x0, y0) * (1 - fx) + at(x1, y0) * fx) * (1 - fy) + (at(x0, y1) * (1 - fx) + at(x1, y1) * fx) * fy;
                out[(size_t(c) * height + y) * width + x] = float(std::round(value) / 127.5 - 1.0);
            }
        }
    }
    return out;
}

// Fixed card text is pale white; level labels are yellow. Discard the darker
// decorative artwork when finding bounds; preserve antialiasing in the primary
// crop and use a binary crop only for uncertain recognition.
inline Image text_crop(const Image& image, bool yellow, bool binary = false) {
    if (image.empty()) return {};
    Image mask(image.width, image.height);
    int left = image.width, top = image.height, right = -1, bottom = -1, count = 0;
    for (int y = 0; y < image.height; ++y) for (int x = 0; x < image.width; ++x) {
        size_t i = (size_t(y) * image.width + x) * 3;
        int b = image.pixels[i], g = image.pixels[i+1], r = image.pixels[i+2];
        bool ink = std::min({b,g,r}) >= 165 || (yellow && r >= 180 && g >= 150 && b < 130);
        if (!ink) continue;
        for (int c = 0; c < 3; ++c) mask.pixels[i+c] = 255;
        left = std::min(left, x); right = std::max(right, x);
        top = std::min(top, y); bottom = std::max(bottom, y); ++count;
    }
    if (count < 6 || right - left < 2 || bottom - top < 3) return Image(16, 16);
    constexpr int border = 3;
    Image trimmed(right-left+1+border*2, bottom-top+1+border*2);
    for (int y = top; y <= bottom; ++y)
        std::copy_n((binary ? mask : image).pixels.data()+(size_t(y)*image.width+left)*3, size_t(right-left+1)*3,
                    trimmed.pixels.data()+(size_t(y-top+border)*trimmed.width+border)*3);
    return trimmed;
}

inline Image digits_crop(const Image& image, bool binary = false) {
    if (image.empty()) return {};
    int end = -1, count = 0;
    for (int y=0; y<image.height; ++y) for (int x=0; x<image.width/2; ++x) {
        size_t i=(size_t(y)*image.width+x)*3;
        if (image.pixels[i+2]>=180 && image.pixels[i+1]>=150 && image.pixels[i]<130) { end=std::max(end,x); ++count; }
    }
    if (count<3 || end<0) return Image(16,16);
    auto digits = crop(image, {double(end+1)/image.width,0,double(image.width-end-1)/image.width,1});
    return text_crop(digits,false,binary);
}
