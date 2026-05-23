import { read_cache, rebuild_graph } from "@mixed/shared";

export function rebuildGraphView() {
  return rebuild_graph(read_cache(["web", "ui"]));
}
