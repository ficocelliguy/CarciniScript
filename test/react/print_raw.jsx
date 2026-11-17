function Boxed({ label, x }) {
  return <i title={label}>{x}</i>;
}
export async function main(ns) {
  ns.printRaw("plain text");
  ns.printRaw("42");
  ns.printRaw(2.5);
  ns.printRaw(["a", "b"]);
  ns.tprintRaw(<b>bold</b>);
  const none = null;
  ns.printRaw(
    <p>
      <Boxed label="char" x="x" />
      <Boxed label="list" x={[1, null, 3]} />
      {none}
      {"owned"}
    </p>,
  );
}
