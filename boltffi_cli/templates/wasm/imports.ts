import { WasmBindgenModule } from {{ runtime_package|json }};
import { __wbg_set_wasm } from {{ glue_module|json }};
{% for (module, names) in modules %}import * as importedModule{{ loop.index0 }} from {{ module|json }};
{% endfor %}
export const wasmBindgen = new WasmBindgenModule({
{% for (module, names) in modules %}{% let module_index = loop.index0 %}  [{{ module|json }}]: {
{% for name in names %}    [{{ name|json }}]: importedModule{{ module_index }}[{{ name|json }}],
{% endfor %}  },
{% endfor %}}, __wbg_set_wasm);
