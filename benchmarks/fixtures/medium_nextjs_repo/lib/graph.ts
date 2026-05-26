import { loadCache } from "@/lib/cache";

export function rebuild_graph(seed: string[]) {
  return loadCache(seed).map((value, index) => ({
    id: `${value}-${index}`,
    edges: index + 1,
  }));
}
