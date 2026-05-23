import { NextResponse } from "next/server";
import { loadAdminGraph } from "../../lib/graph";

export async function POST() {
  return NextResponse.json({ nodes: loadAdminGraph().length });
}
