// 主窗口里各页之间的跳转，例如从当前对局点玩家名字打开他的档案。

import { ref } from "vue";

export type Tab = "match" | "history" | "players" | "reminders" | "settings";
export const tabs: Tab[] = ["match", "history", "players", "reminders", "settings"];

// 地址里的 ?tab= 与 ?player= 可以指定初始页面（预览用）
const params = new URLSearchParams(location.search);
const requestedPlayer = Number(params.get("player"));
const requestedMatch = Number(params.get("match"));

export const currentTab = ref<Tab>(tabs.find((t) => t === params.get("tab")) ?? "match");
export const playerSource = ref<"local" | "batrace">("local");
export const selectedPlayer = ref<number | null>(Number.isFinite(requestedPlayer) && requestedPlayer > 0 ? requestedPlayer : null);

export const selectedMatch = ref<number | null>(Number.isFinite(requestedMatch) && requestedMatch > 0 ? requestedMatch : null);

export const historyPlayer = ref<{ id: number; name: string } | null>(null);

export function openPlayerHistory(id: number, name: string) {
  historyPlayer.value = { id, name };
  selectedMatch.value = null;
  currentTab.value = "history";
}

export function openMatch(id: number) {
  historyPlayer.value = null;
  selectedMatch.value = id;
  currentTab.value = "history";
}

export function openPlayer(id: number) {
  playerSource.value = "local";
  selectedPlayer.value = id;
  currentTab.value = "players";
}

export function openPlayerSearch() {
  selectedPlayer.value = null;
  playerSource.value = "batrace";
  currentTab.value = "players";
}
