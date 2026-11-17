// 主窗口里各页之间的跳转，例如从当前对局点玩家名字打开他的档案。

import { ref } from "vue";

export type Tab = "match" | "players" | "reminders" | "settings";
export const tabs: Tab[] = ["match", "players", "reminders", "settings"];

// 地址里的 ?tab= 与 ?player= 可以指定初始页面（预览用）
const params = new URLSearchParams(location.search);
const requestedPlayer = Number(params.get("player"));

export const currentTab = ref<Tab>(tabs.find((t) => t === params.get("tab")) ?? "match");
export const selectedPlayer = ref<number | null>(Number.isFinite(requestedPlayer) && requestedPlayer > 0 ? requestedPlayer : null);

export function openPlayer(id: number) {
  selectedPlayer.value = id;
  currentTab.value = "players";
}
