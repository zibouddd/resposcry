import { rebuild_graph } from "@/lib/graph";

export async function readGraphSummary() {
  return rebuild_graph(["page", "query"]).map((node) => node.id);
}
