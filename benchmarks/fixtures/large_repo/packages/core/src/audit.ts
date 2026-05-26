import { rebuild_graph } from "@large/graph/rebuild";
import { readCache } from "@large/graph/cache";

export function auditGraph() {
  return rebuild_graph(readCache(["audit", "service", "report"]));
}
