import { ref } from "vue";
import type { RecentStreamIdentity } from "../types/graph";

// Lets another view (e.g. Dashboard's "Recently seen" widget) hand off an
// identity to Rules.vue and have it auto-open a pre-filled create-rule modal
// after navigating there, without wiring a router/query-param mechanism.
const pendingIdentity = ref<RecentStreamIdentity | null>(null);

export function useRuleDraft() {
  function requestRuleForIdentity(entry: RecentStreamIdentity) {
    pendingIdentity.value = entry;
  }

  function consumePendingIdentity(): RecentStreamIdentity | null {
    const entry = pendingIdentity.value;
    pendingIdentity.value = null;
    return entry;
  }

  return { requestRuleForIdentity, consumePendingIdentity };
}
