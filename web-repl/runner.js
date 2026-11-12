// Module worker that runs one compiled program, so a runaway `loop {}` can be terminated without
// freezing the page. Messages out: { type: "out" | "err" | "ns" | "call" | "panic" | "error" | "done", ... }
const post = (type, data = {}) => self.postMessage({ type, ...data });

// Generic mock: any `ns.foo.bar(...)` call is logged and returns a plausible value. Methods that
// are async in Bitburner return Promises.
function genericNs() {
  const asyncFns = new Set(["hack", "grow", "weaken", "sleep", "asleep", "share", "prompt", "wget", "nextWrite"]);
  // A little server state so typical grow/weaken/hack loops terminate.
  const server = { money: 1_750_000, maxMoney: 2_500_000, security: 7.5, minSecurity: 5 };
  const values = {
    getServerMaxMoney: () => server.maxMoney,
    getServerMoneyAvailable: () => server.money,
    getServerSecurityLevel: () => server.security,
    getServerMinSecurityLevel: () => server.minSecurity,
    getServerRequiredHackingLevel: () => 10,
    getHackingLevel: () => 42,
    getHostname: () => "home",
    hack: () => {
      const stolen = server.money * 0.1;
      server.money -= stolen;
      server.security += 0.002;
      return stolen;
    },
    grow: () => {
      const before = server.money;
      server.money = Math.min(server.maxMoney, server.money * 1.25);
      server.security += 0.004;
      return server.money / before;
    },
    weaken: () => {
      const before = server.security;
      server.security = Math.max(server.minSecurity, server.security - 0.05);
      return before - server.security;
    },
    sleep: () => true,
  };
  const make = (path) =>
    new Proxy(function () {}, {
      get(_, prop) {
        if (typeof prop === "symbol" || prop === "then") return undefined;
        return make(path ? `${path}.${prop}` : prop);
      },
      apply(_, __, args) {
        const name = path.split(".").pop();
        if (name === "tprint" || name === "print") {
          post("ns", { text: args.join("") });
          return undefined;
        }
        post("call", { text: `ns.${path}(${args.map((a) => JSON.stringify(a)).join(", ")})` });
        const v = values[name]?.() ?? 0;
        if (!asyncFns.has(name)) return v;
        const ms = name === "sleep" ? Math.min(Number(args[0]) || 0, 200) : 50;
        return new Promise((r) => setTimeout(() => r(v), ms));
      },
    });
  return make("");
}

// The scripted mock used by test/ns/hack.rs (mirrors test/run.mjs).
function hackFixtureNs() {
  const state = { security: 10, minSecurity: 1, money: 100, maxMoney: 1000 };
  const log = (text) => post("ns", { text });
  return {
    getServerMaxMoney: () => state.maxMoney,
    getServerMinSecurityLevel: () => state.minSecurity,
    getServerSecurityLevel: () => state.security,
    getServerMoneyAvailable: () => state.money,
    async weaken(t) {
      log(`weaken ${t}`);
      state.security = 1;
      return 9;
    },
    async grow(t) {
      log(`grow ${t}`);
      state.money = 1000;
      return 10;
    },
    async hack(t) {
      log(`hack ${t}`);
      return 1234.5678;
    },
    tprint: (s) => log(s),
  };
}

self.onmessage = async ({ data }) => {
  const { code, sourceMap, filename, runtimeUrl, mock } = data;
  try {
    await import(runtimeUrl);
    __crs.$setOutput(
      (text) => post("out", { text }),
      (text) => post("err", { text }),
    );
    const bytes = new TextEncoder().encode(sourceMap);
    let bin = "";
    for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    const map = btoa(bin);
    const src = `${code}\n//# sourceURL=carciniscript:///${filename}.js\n//# sourceMappingURL=data:application/json;base64,${map}\n`;
    const url = URL.createObjectURL(new Blob([src], { type: "text/javascript" }));
    const mod = await import(url);
    URL.revokeObjectURL(url);
    if (typeof mod.main !== "function") throw new Error("no `main` function");
    const ns = mock === "hack-fixture" ? hackFixtureNs() : genericNs();
    const start = performance.now();
    await mod.main(ns);
    __crs.$flush();
    post("done", { exitCode: 0, ms: performance.now() - start });
  } catch (e) {
    globalThis.__crs?.$flush();
    if (e?.name === "CarciniPanic") {
      post("panic", { message: e.panicMessage, location: e.location });
      post("done", { exitCode: 101 });
    } else {
      post("error", { message: String(e?.message ?? e), stack: String(e?.stack ?? "") });
      post("done", { exitCode: 1 });
    }
  }
};
