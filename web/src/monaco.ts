// Monaco servido desde el propio bundle (sin CDN): la demo funciona sin red
// externa y el despliegue (F7.7) no depende de jsdelivr.
import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";

self.MonacoEnvironment = {
  getWorker(_id: string, etiqueta: string) {
    return etiqueta === "json" ? new JsonWorker() : new EditorWorker();
  },
};

// El servidor es quien valida el esquema (meta-esquema propio del subconjunto):
// el JSON de Monaco solo resalta la sintaxis, sin esquemas remotos.
monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
  validate: true,
  allowComments: false,
  enableSchemaRequest: false,
  schemas: [],
});

loader.config({ monaco });
