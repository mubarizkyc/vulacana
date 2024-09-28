# vulacana
When you want to ensure that your transaction is processed, you can send the same request to multiple providers and let the validators determine which one arrives first. Keep in mind that some RPC methods work with certain providers while others do not, but you can pass this information along.

Since all RPC providers can experience issues, sol-shotty helps you get faster responses when a specific RPC provider is slow or completely down, without requiring any reconfiguration!

Running
```console
cargo run
```
Example
```console
curl -X POST http://localhost:8080/proxy \
    -H "Content-Type: application/json" \
    -d '{ "jsonrpc": "2.0", "id": 1, "method": "getBalance", "params": ["CiNrZRUGGMpMjGKcNZ1QmedTiQ5RUzkyz2V6mTPkzT3C"] }'

```
You can use free as well as paid RPC providers 

Free Providers

[Helius](https://www.helius.dev/solana-apis)

[HelloMoon](https://docs.hellomoon.io/reference/hello-moon-rpc)
