import { ref } from "vue";

const searchQuery = ref("");

export function useGlobalSearch() {
  return { searchQuery };
}
