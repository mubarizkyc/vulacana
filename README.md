# vulacana
Example Usage
Running
```console
cargo run
```
```console
curl -X POST http://localhost:8080/proxy \
    -H "Content-Type: application/json" \
    -d '{ "jsonrpc": "2.0", "id": 1, "method": "getBalance", "params": ["CiNrZRUGGMpMjGKcNZ1QmedTiQ5RUzkyz2V6mTPkzT3C"] }'

```
