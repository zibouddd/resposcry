import { loadAdminGraph } from "../lib/graph";

export default function AdminPage() {
  return <pre>{JSON.stringify(loadAdminGraph(), null, 2)}</pre>;
}
