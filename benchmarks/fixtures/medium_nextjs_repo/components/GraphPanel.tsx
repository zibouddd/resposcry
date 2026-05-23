export function GraphPanel({ summary }: { summary: string[] }) {
  return (
    <section>
      <h1>Graph summary</h1>
      <ul>{summary.map((item) => <li key={item}>{item}</li>)}</ul>
    </section>
  );
}
