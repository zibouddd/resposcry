import { readCache } from "@large/graph/cache";
import { rebuild_graph } from "@large/graph/rebuild";

export function loadAdminGraph() {
  return rebuild_graph(readCache(["admin", "route", "page"]));
}
