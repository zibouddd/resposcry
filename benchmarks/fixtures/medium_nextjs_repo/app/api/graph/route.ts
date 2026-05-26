import { NextResponse } from "next/server";
import { rebuild_graph } from "@/lib/graph";

export async function GET() {
  const graph = rebuild_graph(["api", "route", "cache"]);
  return NextResponse.json({ nodes: graph.length });
}
