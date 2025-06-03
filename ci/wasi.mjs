import { WASI } from "node:wasi";
import { readFile } from "node:fs/promises";

const wasi = new WASI({
    version: process.argv[2],
    args: process.argv.slice(3),
    env: process.env,
    preopens: {
        "/": "."
    },
});

const wasm = await WebAssembly.compile(await readFile(process.argv[3]));
const instance = await WebAssembly.instantiate(wasm, wasi.getImportObject());

process.exit(wasi.start(instance));
