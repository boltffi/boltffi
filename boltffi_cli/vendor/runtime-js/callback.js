const CALLBACK_NAMESPACE_START = 0x80000000;
export class CallbackRegistry {
    constructor(name) {
        this.entries = new Map();
        this.nextHandle = CALLBACK_NAMESPACE_START;
        this.name = name;
    }
    register(value) {
        const handle = this.nextHandle;
        this.nextHandle = (handle + 1) >>> 0;
        if (this.entries.has(handle)) {
            throw new Error(`${this.name} callback handle namespace exhausted`);
        }
        this.entries.set(handle, { value, references: 1 });
        return handle;
    }
    get(handle) {
        const key = handle >>> 0;
        const entry = this.entries.get(key);
        if (!entry) {
            throw new Error(`${this.name} callback handle ${key} not found`);
        }
        return entry.value;
    }
    retain(handle) {
        const key = handle >>> 0;
        const entry = this.entries.get(key);
        if (!entry) {
            throw new Error(`cannot retain unknown ${this.name} callback handle ${key}`);
        }
        entry.references += 1;
        return key;
    }
    release(handle) {
        const key = handle >>> 0;
        const entry = this.entries.get(key);
        if (!entry) {
            throw new Error(`cannot release unknown ${this.name} callback handle ${key}`);
        }
        entry.references -= 1;
        if (entry.references === 0) {
            this.entries.delete(key);
        }
    }
}
//# sourceMappingURL=callback.js.map