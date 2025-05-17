```bash
lncli --network testnet getinfo

lncli --network testnet getnetworkinfo

lncli --network testnet newaddress p2wkh

lncli --network testnet walletbalance

lncli --network testnet listpeers

lncli --network testnet getnodeinfo <pub-key>

lncli --network testnet connect <node-pub-key@ip:port>

lncli --network testnet openchannel <pub-key> <amount>

lncli --network testnet pendingchannels

lncli --network testnet listchannels
```

### project

```bash
# 1) Generate a random 32‑byte preimage, hex‑encoded
#    We build a hex string by getting 32 random bytes (0–255) and formatting each as two hex digits.
$X = -join (1..32 | ForEach-Object { "{0:x2}" -f (Get-Random -Maximum 256) })
Write-Host "Preimage (X): $X"

# 2) Convert hex string X into a byte array
$bytes = for ($i = 0; $i -lt $X.Length; $i += 2) {
  [Convert]::ToByte($X.Substring($i, 2), 16)
}

# 3) Compute SHA‑256 hash of that byte array
$sha = [System.Security.Cryptography.SHA256]::Create()
$hashBytes = $sha.ComputeHash($bytes)

# 4) Convert the hash bytes back into a hex string
$H = ($hashBytes | ForEach-Object { $_.ToString("x2") }) -join ""
Write-Host "Hash     (H): $H"
```

```bash
lncli --network testnet addholdinvoice $H --amt=1000 --memo="My custom invoice"

lncli --network testnet lookupinvoice $H
```
