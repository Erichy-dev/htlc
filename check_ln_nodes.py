#!/usr/bin/env python3
import json
import subprocess
import re
import sys

def run_command(command):
    try:
        process = subprocess.Popen(
            command,
            shell=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            encoding='utf-8',
            errors='replace'
        )
        stdout, stderr = process.communicate()
        
        if process.returncode != 0:
            print(f"Command failed with error: {stderr}", file=sys.stderr)
            return None
        return stdout
    except Exception as e:
        print(f"Error running command: {e}", file=sys.stderr)
        return None

def get_graph_info():
    # Get the network graph
    print("Fetching network graph...", file=sys.stderr)
    graph_output = run_command("lncli --network=testnet describegraph")
    if not graph_output:
        return []
    
    # Parse the JSON output
    try:
        graph_data = json.loads(graph_output)
        return graph_data.get('edges', [])
    except json.JSONDecodeError as e:
        print(f"Error parsing graph data: {e}", file=sys.stderr)
        return []

def get_node_info(pub_key):
    node_output = run_command(f"lncli --network=testnet getnodeinfo {pub_key}")
    if not node_output:
        return None
    
    try:
        return json.loads(node_output)
    except json.JSONDecodeError as e:
        print(f"Error parsing node data for {pub_key}: {e}", file=sys.stderr)
        return None

def has_valid_address(addresses):
    for addr in addresses:
        # Check if address has a port number and is not an onion address
        addr_str = addr.get('addr', '')
        if ':' in addr_str and '.onion:' not in addr_str:
            return True
    return False

def find_all_non_tor_nodes(node_pubkeys):
    non_tor_nodes = []
    for i, pubkey in enumerate(node_pubkeys, 1):
        print(f"\rChecking node {i}/{len(node_pubkeys)}...", end='', file=sys.stderr)
        node_info = get_node_info(pubkey)
        
        if node_info and 'node' in node_info:
            addresses = node_info['node'].get('addresses', [])
            if has_valid_address(addresses):
                non_tor_nodes.append({
                    'pubkey': pubkey,
                    'alias': node_info['node'].get('alias', 'Unknown'),
                    'addresses': [addr['addr'] for addr in addresses]
                })
    print("\n")  # New line after progress
    return non_tor_nodes

def main():
    # Get all edges from the graph
    edges = get_graph_info()
    
    # Collect unique node pubkeys
    node_pubkeys = set()
    for edge in edges:
        node_pubkeys.add(edge['node1_pub'])
        node_pubkeys.add(edge['node2_pub'])
    
    print(f"Found {len(node_pubkeys)} unique nodes")
    print("\nSearching for non-Tor nodes...")
    
    non_tor_nodes = find_all_non_tor_nodes(node_pubkeys)
    
    if non_tor_nodes:
        print(f"\nFound {len(non_tor_nodes)} non-Tor nodes:")
        print("===================")
        
        for i, node in enumerate(non_tor_nodes, 1):
            print(f"\n{i}. Alias: {node['alias']}")
            print(f"   Pubkey: {node['pubkey']}")
            print("   Addresses:")
            for addr in node['addresses']:
                print(f"     - {addr}")
    else:
        print("\nNo non-Tor nodes found with valid addresses.")

if __name__ == "__main__":
    main() 