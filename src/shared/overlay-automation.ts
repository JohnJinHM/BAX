import type { MatchView } from "./bindings/MatchView";
import type { OverlayAppearance } from "./settings";

/** 只管理展开状态；手动操作保留到下一阶段，不改变比赛状态或用户偏好。 */
export class OverlayAutomation {
  private identity = "";
  private stage = "";

  update(match: MatchView | null, settings: OverlayAppearance, now: number): boolean | null {
    if (!settings.autoExpand || !match) {
      this.identity = ""; this.stage = "";
      return null;
    }
    const identity = `${match.matchId}:${match.discoveredAt}`;
    const fresh = identity !== this.identity;
    if (fresh) {
      this.identity = identity; this.stage = "";
    }
    const remaining = match.clock ? match.clock.startAt - now : null;
    const threshold = Math.max(0, Math.min(180, settings.collapseBeforeBattleSecs)) * 1000;
    // Rust 的 RoomExited 已把连接改为 disconnected；退出优先于赛后展示。
    const stage = match.connection === "disconnected" ? "left"
      : match.phase === "ended" ? "ended"
      : match.phase === "inBattle" || (match.phase === "deployment" && remaining != null && remaining <= threshold) ? "collapsed"
      : "pregame";
    if (stage === this.stage) return null;
    this.stage = stage;
    if (stage === "left" || stage === "collapsed") return false;
    if (stage === "ended") {
      // 重启应用时不重播旧比赛的结束提示。
      return !fresh || (match.endedAt != null && now - match.endedAt < 120_000);
    }
    return true;
  }
}
