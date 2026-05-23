import { rebuildDashboardGraph } from "../../lib/graph";

export default function GraphPage() {
  return <pre>{JSON.stringify(rebuildDashboardGraph(), null, 2)}</pre>;
}
