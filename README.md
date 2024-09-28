# vulacana
Running
```console
cargo run
```
Usage
```console
curl -X POST http://localhost:8080/proxy \
    -H "Content-Type: application/json" \
    -d '{ "jsonrpc": "2.0", "id": 1, "method": "getBalance", "params": ["CiNrZRUGGMpMjGKcNZ1QmedTiQ5RUzkyz2V6mTPkzT3C"] }'

```
You can use free as well as paid RPC providers 

Free Providers

[Helius](https://www.helius.dev/solana-apis)

[HelloMoon](https://docs.hellomoon.io/reference/hello-moon-rpc)
