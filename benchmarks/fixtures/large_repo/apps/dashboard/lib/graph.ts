import { selectGraph } from "@large/graph/selectors";
import { rebuild_graph } from "@large/graph/rebuild";

export function rebuildDashboardGraph() {
  return rebuild_graph(selectGraph(["dashboard", "widgets", "alerts"]));
}
