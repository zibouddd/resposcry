import { rebuildDashboardGraph } from "../lib/graph";

export default function DashboardPage() {
  const graph = rebuildDashboardGraph();
  return <pre>{JSON.stringify(graph, null, 2)}</pre>;
}
