export class WasmBindgenModule {
    constructor(imports, setWasm) {
        this.imports = imports;
        this.setWasm = setWasm;
        this.state = "idle";
    }
    acquire() {
        if (this.state !== "idle") {
            throw new Error(`Cannot initialize wasm-bindgen glue in state ${this.state}; each glue module owns one Wasm instance`);
        }
        this.state = "loading";
    }
    initialize(exports) {
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
    release() {
        if (this.state === "loading")
            this.state = "idle";
    }
}
//# sourceMappingURL=wasm-bindgen.js.map