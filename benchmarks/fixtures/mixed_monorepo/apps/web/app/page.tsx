import { rebuildGraphView } from "../lib/graph";

export default function Page() {
  const graph = rebuildGraphView();
  return <pre>{JSON.stringify(graph, null, 2)}</pre>;
}
