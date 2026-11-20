# sandbox

Imagen Docker aislada para compilar y ejecutar el código generado (F7.3, D68). Detalle en
[docs/fase7/sandbox.md](../docs/fase7/sandbox.md).

```text
docker build -t validador-sandbox:1 sandbox
```

La usa `server::sandbox` (`server/src/sandbox.rs`). Sus pruebas con contenedores reales corren con
`scripts/test-sandbox.ps1` o `make test-sandbox`.
