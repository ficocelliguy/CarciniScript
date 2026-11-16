function row(n) {
  return <li key={n}>item {n}</li>;
}
export async function main(ns) {
  function Counter() {
    const [count, setCount] = React.useState(() => 1);
    const [log, setLog] = React.useState(() => []);
    return (
      <div id="counter">
        <button className="inc" onClick={() => setCount((c) => c + 1)}>+1</button>
        <button
          className="reset"
          onClick={() => {
            setCount(0);
            setLog((l) => [...l, "reset"]);
          }}
        >
          reset
        </button>
        <span data-count={count}>{count}</span>
        <ul>{[...Array(count).keys()].map(row)}</ul>
        {count === 0 ? <em>empty</em> : null}
        <ol>{log.map((l) => <li key={l}>{l}</li>)}</ol>
      </div>
    );
  }
  ns.printRaw(<Counter />);
}
