import base64, codecs, json, requests

REST_HOST = 'localhost:8080'
MACAROON_PATH = 'LND_DIR/data/chain/bitcoin/regtest/admin.macaroon'
TLS_PATH = 'LND_DIR/tls.cert'

url = f'https://{REST_HOST}/v1/invoices/subscribe'
macaroon = codecs.encode(open(MACAROON_PATH, 'rb').read(), 'hex')
headers = {'Grpc-Metadata-macaroon': macaroon}
r = requests.get(url, headers=headers, stream=True, verify=TLS_PATH)
for raw_response in r.iter_lines():
  json_response = json.loads(raw_response)
  print(json_response)
# {
#    "memo": <string>,
#    "r_preimage": <bytes>,
#    "r_hash": <bytes>,
#    "value": <int64>,
#    "value_msat": <int64>,
#    "settled": <bool>,
#    "creation_date": <int64>,
#    "settle_date": <int64>,
#    "payment_request": <string>,
#    "description_hash": <bytes>,
#    "expiry": <int64>,
#    "fallback_addr": <string>,
#    "cltv_expiry": <uint64>,
#    "route_hints": <RouteHint>,
#    "private": <bool>,
#    "add_index": <uint64>,
#    "settle_index": <uint64>,
#    "amt_paid": <int64>,
#    "amt_paid_sat": <int64>,
#    "amt_paid_msat": <int64>,
#    "state": <InvoiceState>,
#    "htlcs": <InvoiceHTLC>,
#    "features": <FeaturesEntry>,
#    "is_keysend": <bool>,
#    "payment_addr": <bytes>,
#    "is_amp": <bool>,
#    "amp_invoice_state": <AmpInvoiceStateEntry>,
# }