import type { MatchView } from "./bindings/MatchView";
import type { OverlayAppearance } from "./settings";

/** 只管理展开状态；手动操作保留到下一阶段，不改变比赛状态或用户偏好。 */
export class OverlayAutomation {
  private identity = "";
  private stage = "";
  private resultSeen = false;
  private collapseAt: number | null = null;

  manual() { this.collapseAt = null; }

  update(match: MatchView | null, settings: OverlayAppearance, now: number, hasResult: boolean): boolean | null {
    if (!settings.autoExpand || !match) {
      this.identity = ""; this.stage = ""; this.collapseAt = null; this.resultSeen = false;
      return null;
    }
    const identity = `${match.matchId}:${match.discoveredAt}`;
    const fresh = identity !== this.identity;
    if (fresh) {
      this.identity = identity; this.stage = ""; this.collapseAt = null; this.resultSeen = false;
    }
    const remaining = match.clock ? match.clock.startAt - now : null;
    const threshold = Math.max(0, Math.min(180, settings.collapseBeforeBattleSecs)) * 1000;
    const stage = match.phase === "ended" ? "ended"
      : match.phase === "inBattle" || (match.phase === "deployment" && remaining != null && remaining <= threshold) ? "collapsed"
      : "pregame";
    let action: boolean | null = null;
    const duration = Math.max(3, Math.min(60, settings.resultDisplaySecs)) * 1000;
    if (stage !== this.stage) {
      this.stage = stage;
      this.collapseAt = null;
      action = stage !== "collapsed";
      if (stage === "ended") {
        // 重启应用时不重播旧比赛的结束提示。
        const recent = match.endedAt != null && now - match.endedAt < 120_000;
        action = !fresh || recent;
        this.collapseAt = action ? now + duration : null;
        this.resultSeen = hasResult || !action;
      }
    }
    if (stage === "ended" && hasResult && !this.resultSeen) {
      this.resultSeen = true;
      this.collapseAt = now + duration;
      action = true;
    }
    if (this.collapseAt != null && now >= this.collapseAt) {
      this.collapseAt = null;
      action = false;
    }
    return action;
  }
}
