import { onBeforeUnmount, ref, watch } from "vue";
import type { Ref } from "vue";
import type { Snapshot } from "./bindings/Snapshot";
import type { MatchReportView } from "./bindings/MatchReportView";
import { commands, errorText } from "./engine";

export function useCurrentReport(snapshot: Ref<Snapshot | null>) {
  const report = ref<MatchReportView | null>(null);
  const failure = ref<string | null>(null);
  let ticket = 0;
  watch([() => snapshot.value?.current?.matchId, () => snapshot.value?.current?.phase,
    () => snapshot.value?.archive.seq], async () => {
    const request = ++ticket;
    const match = snapshot.value?.current;
    if (report.value?.analysis.matchId !== match?.matchId || match?.phase !== "ended") report.value = null;
    failure.value = null;
    if (!match || match.phase !== "ended") return;
    try {
      const result = await commands.matchReport(match.matchId);
      if (request === ticket) report.value = result;
    } catch (error) {
      if (request === ticket) failure.value = errorText(error);
    }
  }, { immediate: true });
  onBeforeUnmount(() => { ++ticket; });
  return { report, failure };
}
