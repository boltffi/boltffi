const CALLBACK_NAMESPACE_START = 0x80000000;

interface CallbackEntry<T> {
  value: T;
  references: number;
}

export class CallbackRegistry<T> {
  private readonly name: string;
  private readonly entries = new Map<number, CallbackEntry<T>>();
  private nextHandle = CALLBACK_NAMESPACE_START;

  constructor(name: string) {
    this.name = name;
  }

  register(value: T): number {
    const handle = this.nextHandle;
    this.nextHandle = (handle + 1) >>> 0;
    if (this.entries.has(handle)) {
      throw new Error(`${this.name} callback handle namespace exhausted`);
    }
    this.entries.set(handle, { value, references: 1 });
    return handle;
  }

  get(handle: number): T {
    const key = handle >>> 0;
    const entry = this.entries.get(key);
    if (!entry) {
      throw new Error(`${this.name} callback handle ${key} not found`);
    }
    return entry.value;
  }

  retain(handle: number): number {
    const key = handle >>> 0;
    const entry = this.entries.get(key);
    if (!entry) {
      throw new Error(`cannot retain unknown ${this.name} callback handle ${key}`);
    }
    entry.references += 1;
    return key;
  }

  release(handle: number): void {
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
