# 3. RPC Protocol

### Overview

JSON-RPC 2.0 over HTTP. Server listens on `127.0.0.1:{port}` (default 8080).

| Endpoint  | Method | Purpose                           |
| --------- | ------ | --------------------------------- |
| `/rpc`    | POST   | Main JSON-RPC handler             |
| `/health` | POST   | Health check → `{"status": "ok"}` |

Request format:

```json
{"jsonrpc": "2.0", "method": "server.info", "params": {}, "id": 1}
```

Response format:

```json
{"jsonrpc": "2.0", "result": {...}, "id": 1}
```

Error format:

```json
{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": 1}
```

### Client Usage (curl)

```bash
# Health check
curl -s -X POST http://127.0.0.1:8080/health

# JSON-RPC: one canonical call — swap method/params as needed (full reference below)
curl -s -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"check","params":{"uri":"/path/to/file.mc","libs":["mcode"]}}'

# Representative params (method tables below are authoritative; server.methods/caps list live methods):
#   parse / check   {"uri":"/path/to/file.mc","libs":["mcode"]}   # content: for inline snippets
#   sem             {"uri":"/path/to/file.mc"}                     # LSP semantic tokens+symbols
#   diagnostics     {"uri":"/path/to/file.mc"}
#   def / refs      {"name":"RES"}                                 # go-to-def / references
#   build.full      {"entry":"/path/to/file.mc","top":"TOP","libs":["mcode"]}
#   show.*          {"name":"RES"}                                 # show.component/dump/all
#   lib.*           {}                                              # lib.list / info / load / unload
```

### RPC Methods Reference

#### Discovery

| Method           | Params | Returns                             |
| ---------------- | ------ | ----------------------------------- |
| `server.info`    | —      | Server version, uptime, loaded libs |
| `server.methods` | —      | List of all registered methods      |
| `caps`           | —      | Self-describing capabilities        |

#### Workspace

| Method             | Params | Returns                    |
| ------------------ | ------ | -------------------------- |
| `init`             | —      | Initialize workspace       |
| `load_project`     | `uri`  | Load project entry file    |
| `add_file`         | `uri`  | Add file to workspace      |
| `remove_file`      | `uri`  | Remove file from workspace |
| `set_project_root` | `path` | Set project root directory |
| `set_system_root`  | `path` | Set system library root    |

#### Parse / Build

| Method       | Params                         | Returns            |
| ------------ | ------------------------------ | ------------------ |
| `parse`      | `uri`, `code?`, `libs?`        | Parse result       |
| `check`      | `uri`                          | Diagnostics        |
| `build.full` | `uri`, `top?`                  | Full build result  |
| `extract`    | `kind`, `uri`, `top?`, `name?` | Extracted entities |

#### Show / Inspect

| Method                | Params          | Returns                     |
| --------------------- | --------------- | --------------------------- |
| `show.all`            | `file?`         | All entities (list only)    |
| `show.component`      | `name?`         | Component list or detail    |
| `show.component.list` | —               | Flat list of all components |
| `show.module`         | `name?`         | Module list or detail       |
| `show.module.list`    | —               | Flat list of all modules    |
| `show.interface`      | `name?`         | Interface list or detail    |
| `show.interface.list` | —               | Flat list of all interfaces |
| `show.enum`           | `name?`         | Enum list or detail         |
| `show.enum.list`      | —               | Flat list of all enums      |
| `show.net`            | `name?`         | Net list or detail          |
| `show.net.list`       | —               | Flat list of all nets       |
| `show.pins`           | `name`          | Pin definitions             |
| `show.ports`          | `name`          | Port definitions            |
| `show.ports.list`     | —               | Flat list of all ports      |
| `show.labels`         | `name`          | Labels of a module          |
| `show.instances`      | `name`, `file?` | Instance list with kinds    |
| `show.nets`           | `name`, `file?` | Net list                    |
| `show.attrs`          | `name`          | Attribute list              |
| `show.funcs`          | `name`          | Function list               |
| `show.params`         | `name`          | Parameter list              |
| `show.roles`          | `name`          | Role definitions            |
| `show.values`         | `name`          | Enum values                 |
| `show.dump`           | `name`          | Full entity dump            |
| `show.dump.all`       | —               | Dump all loaded entities    |
| `show.file`           | `uri`           | All definitions in file     |
| `show.files`          | —               | All loaded files            |

#### Semantics / LSP

| Method            | Params                                           | Returns                                                    |
| ----------------- | ------------------------------------------------ | ---------------------------------------------------------- |
| `sem`             | `uri`, `content?`                                | Semantic tokens + symbols                                  |
| `diagnostics`     | `uri`                                            | File diagnostics                                           |
| `project_symbols` | —                                                | Project-wide symbol index                                  |
| `def`             | `name`                                           | Go-to-definition by name                                   |
| `refs`            | `name`                                           | Find all references by name                                |
| `hover`           | `name`, `uri`                                    | Hover tooltip info                                         |
| `completion`      | `uri`, `line`, `column`                          | Code completions at position                               |
| `defs.search`     | `pattern`, `kind?`, `regex?`, `fuzzy?`, `limit?` | Text/regex/fuzzy search across definitions                 |
| `defs.query`      | `expr`, `limit?`                                 | Structured DSL query (e.g. `kind=component AND name=RES*`) |
| `lookup`          | `name`                                           | Lookup by name                                             |
| `lookup_sub`      | `parentUri`, `kind`, `name`                      | Scoped lookup                                              |
| `lookup_all`      | —                                                | All lookup entries                                         |
| `erc`             | `uri?`, `top?`                                   | Electrical rule check                                      |

#### Library

| Method          | Params  | Returns               |
| --------------- | ------- | --------------------- |
| `lib.list`      | —       | Loaded libraries      |
| `lib.info`      | `name`  | Library metadata      |
| `lib.load`      | `name`  | Load a library        |
| `lib.unload`    | `name`  | Unload a library      |
| `lib.install`   | `path`  | Install library       |
| `lib.uninstall` | `name`  | Uninstall library     |
| `lib.search`    | `query` | Search installed libs |

#### Export / Utility

| Method      | Params               | Returns                |
| ----------- | -------------------- | ---------------------- |
| `export`    | `kind`, `uri`, `top` | Export result          |
| `convert`   | `uri`, `format`      | Convert file           |
| `report`    | `uri?`               | Design report          |
| `explain`   | `code?`              | Error code description |
| `trace.set` | `config`             | Update trace config    |
| `trace.get` | —                    | Current trace config   |

### Error Codes

| Code   | Meaning                    |
| ------ | -------------------------- |
| -32700 | Parse error (invalid JSON) |
| -32600 | Invalid request            |
| -32601 | Method not found           |
| -32602 | Invalid params             |
| -32603 | Internal error             |
| 32100  | I/O or filesystem error    |
| 32101  | Workspace conflict         |
| 32102  | Workspace not found        |
| 32103  | Archive decode failed      |
| 32104  | Unsupported format         |
| 32105  | Entry file not found       |
| 32106  | Dependency not loaded      |
| 32107  | Pass1 or Pass2 failed      |
| 32108  | Build panic                |

***
