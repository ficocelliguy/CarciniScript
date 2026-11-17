function reduce(log, action) {
  switch (action.type) {
    case "add":
      return [...log, action.n];
    case "reset":
      return [];
  }
}
function Badge({ label, count }) {
  return (
    <span className="badge" title={`${label}: ${count}`}>
      {label}{" "}{count}
    </span>
  );
}
export async function main(ns) {
  const Theme = React.createContext("light");
  function Themed() {
    const t = React.useContext(Theme);
    return <em className={`theme-${t}`}>{t}</em>;
  }
  function App() {
    const [log, dispatch] = React.useReducer(reduce, []);
    const total = React.useMemo(() => log.reduce((a, b) => a + b, 0), [log.length]);
    const [last, setLast] = React.useState(() => "none");
    const [count, setCount] = React.useState(() => 0);
    const input = React.useRef(null);
    React.useEffect(() => {
      if (input.current) ns.print(`ref: ${input.current.tagName.toLowerCase()}`);
    }, []);
    const add = React.useCallback((_e) => dispatch({ type: "add", n: 2 }), []);
    const extra = { "data-kind": "extra", title: "spread" };
    const props = { label: "hits", count };
    const empty = null;
    return (
      <Theme.Provider value="dark">
        <div id="app" style={{ color: "red", fontSize: "12px" }}>
          <button onClick={add}>add 2</button>
          <button className="reset" onClick={() => dispatch({ type: "reset" })}>reset</button>
          <button onClick={(e) => setLast(e.type)}>event</button>
          <button onClick={(_e) => setCount((c) => c + 1)}>bump</button>
          <input ref={input} value="x" readOnly={true} disabled={false} />
          <p title={empty} tabIndex={3}>{`[${log.join(", ")}] = ${total}`}{/* a comment */}</p>
          <p>{"last event: "}{last}</p>
          <Badge {...props} />
          <Badge {...{ label: "fixed", count: 0 }} count={7} />
          <span {...extra} key="s">spread</span>
          <Themed />
        </div>
        <Themed />
      </Theme.Provider>
    );
  }
  ns.printRaw(
    <>
      <App />
      <Themed />
    </>,
  );
}
