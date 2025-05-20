import base64
import argparse

def decode_message(encoded_message: str) -> str:
    """Decodes a base64 encoded string."""
    try:
        decoded_bytes = base64.b64decode(encoded_message)
        # Convert to string, ignoring errors, or using 'replace' for problematic characters
        decoded_str = decoded_bytes.decode('utf-8', errors='replace')
        return decoded_str
    except Exception as e:
        return f"Error decoding message: {e}"

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Decode a base64 encoded message.")
    parser.add_argument("-m", "--message", required=True, help="The base64 encoded message string.")
    args = parser.parse_args()

    decoded_string = decode_message(args.message)
    print(decoded_string)
