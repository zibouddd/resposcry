import type { NextRequest } from "next/server";

export function middleware(_request: NextRequest) {
  return Response.json({ ok: true });
}
