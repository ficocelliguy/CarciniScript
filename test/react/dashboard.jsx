function Gauge({ label, value }) {
  const pct = Math.round(value * 100);
  return (
    <div className="gauge" style={{ width: `${pct}%`, backgroundColor: "#0c0" }}>
      {label}{": "}{pct}{"%"}
    </div>
  );
}
function Panel({ title, children }) {
  return (
    <section aria-label={title}>
      <h3>{title}</h3>
      {children}
    </section>
  );
}
export async function main(ns) {
  const servers = [["n00dles", 0.25], ["joesguns", 0.8], ["phantasy", 1.0]];
  function Ticker() {
    const [ticks, setTicks] = React.useState(() => 0);
    React.useEffect(() => {
      const id = setInterval(() => setTicks((t) => t + 1), 10);
      return () => clearInterval(id);
    }, []);
    const note = ticks >= 3 ? "warmed up" : null;
    return <p>{"ticks: "}{ticks}{" "}{note}</p>;
  }
  const rows = servers.map(([host, frac]) => <li key={host}><Gauge label={host} value={frac} /></li>);
  ns.printRaw(
    <Panel title="Targets">
      <ul>{rows}</ul>
      <Ticker />
      <>
        <button onClick={(_e) => {}} disabled>stop</button>
        <br />
      </>
    </Panel>,
  );
}
