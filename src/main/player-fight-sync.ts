import { reactive } from "vue";
import { commands, errorText } from "../shared/engine";

export const DEFAULT_FIGHT_LIMIT = 20;
// Both refresh actions share the archive job and its stop control across profile switches.
export const active = reactive(new Map<number, Promise<void>>());
export const stopping = reactive(new Set<number>());
export const messages = reactive(new Map<number,string>());

export function syncFights(id:number, limit = DEFAULT_FIGHT_LIMIT): Promise<void> {
  const running = active.get(id);
  if (running) return running;
  messages.delete(id);
  const job = commands.syncPlayerFights(id,limit)
    .then(status => { messages.set(id,status); })
    .catch(error => { messages.set(id,errorText(error)); throw error; })
    .finally(() => { active.delete(id); stopping.delete(id); });
  active.set(id,job);
  return job;
}
