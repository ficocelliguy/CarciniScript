function Bar({ label, value }) {
  const pct = Math.round(value * 100);
  return (
    <div className="bar">
      <span>{label}{": "}{pct}{"%"}</span>
      <div style={{ background: "#222", height: "6px" }}>
        <div style={{ background: "#0c0", height: "6px", width: `${pct}%` }}></div>
      </div>
    </div>
  );
}
export async function main(ns) {
  ns.disableLog("ALL");
  ns.ui.openTail();
  const target = "n00dles";
  function Monitor() {
    const [money, setMoney] = React.useState(() => 0);
    const [running, setRunning] = React.useState(() => true);
    const [hacks, setHacks] = React.useState(() => 0);
    React.useEffect(() => {
      const id = running
        ? setInterval(() => {
            setMoney((m) => Math.min(m + 0.125, 1));
            setHacks((h) => h + 1);
          }, 20)
        : null;
      return () => {
        if (id !== null) clearInterval(id);
      };
    }, [running]);
    const label = running ? "pause" : "resume";
    return (
      <div style={{ fontFamily: "monospace", padding: "4px" }}>
        <h4>{"hacking "}{target}</h4>
        <Bar label="money" value={money} />
        <p>{"hacks: "}{hacks}</p>
        <button onClick={(_e) => setRunning((r) => !r)}>{label}</button>
        <button
          onClick={(_e) => {
            setMoney(0);
            setHacks(0);
          }}
        >
          reset
        </button>
        {money >= 1 ? <strong>server drained</strong> : null}
      </div>
    );
  }
  ns.printRaw(<Monitor />);
}
