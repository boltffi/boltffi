export class WasmBindgenModule {
  private state: "idle" | "loading" | "ready" | "failed" = "idle";

  constructor(
    readonly imports: WebAssembly.Imports,
    private readonly setWasm: (exports: WebAssembly.Exports) => void,
  ) {}

  acquire(): void {
    if (this.state !== "idle") {
      throw new Error(`Cannot initialize wasm-bindgen glue in state ${this.state}; each glue module owns one Wasm instance`);
    }
    this.state = "loading";
  }

  initialize(exports: WebAssembly.Exports): void {
    if (this.state !== "loading") {
      throw new Error(`Cannot attach wasm-bindgen glue in state ${this.state}`);
    }
    this.state = "failed";
    this.setWasm(exports);
    const start = exports.__wbindgen_start;
    if (start !== undefined) {
      if (typeof start !== "function") {
        throw new Error("wasm-bindgen start export is not a function");
      }
      start();
    }
    this.state = "ready";
  }

  fail(): void {
    this.state = "failed";
  }
}
