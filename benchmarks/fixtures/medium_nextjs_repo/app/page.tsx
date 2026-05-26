import { GraphPanel } from "@/components/GraphPanel";
import { readGraphSummary } from "@/lib/queries";

export default async function HomePage() {
  const summary = await readGraphSummary();
  return <GraphPanel summary={summary} />;
}
