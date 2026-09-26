{% match target %}
{% when WasmNpmTarget::Nodejs %}
export * from "./{{ module_name }}_node.js";
export { default, initialized } from "./{{ module_name }}_node.js";
{% when WasmNpmTarget::Bundler or WasmNpmTarget::Web %}
import init from "./{{ module_name }}.js";
export * from "./{{ module_name }}.js";
export { default as init } from "./{{ module_name }}.js";
export const initialized = (async () => {
  const response = await fetch(new URL("./{{ module_name }}_bg.wasm", import.meta.url));
  await init(response);
})();
{% endmatch %}
