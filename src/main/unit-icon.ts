import manifest from "./unit-icons.json";

const normalize = (name: string) => name.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
const icons: Record<string, { file: string; names: string[] }> = manifest;
const byName = new Map<string, Set<string>>();
for (const icon of Object.values(icons)) {
  for (const name of icon.names) {
    const key = normalize(name);
    const files = byName.get(key) ?? new Set<string>();
    files.add(icon.file);
    byName.set(key, files);
  }
}

export function unitIconFile(id: number, name: string): string | undefined {
  const key = normalize(name);
  const icon = icons[id];
  // 目录版本可能不同：ID 命中也核对名称；名称重名时不猜测图标。
  if (icon?.names.some((candidate) => normalize(candidate) === key)) return icon.file;
  const files = byName.get(key);
  return files?.size === 1 ? [...files][0] : undefined;
}
