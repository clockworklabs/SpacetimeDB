# A simple testing tool for talking to SpacetimeDB using a websocket.
# Can be used to send arbitrary payloads either from command line or file, with a configurable pause
# between messages.
# Results from the server are passed through the protobuf parser and printed to stdout.
# Everything else (error messages etc.) goes on stderr.

# Run setup.sh in this directory to install the pre-requisites and compile the protobuf.

import argparse
import sys
import time
import urllib.parse

import websocket

import WebSocket_pb2


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)


def ws_send(url, payload_lines, pause):
    eprint("Connecting to %s ..." % url.geturl())
    extra_headers = {"sec-websocket-protocol": "v1.bin.spacetimedb"}
    ws = websocket.WebSocket()
    ws.connect(url.geturl(), header=extra_headers, timeout=pause)
    eprint("Connected...")
    ws.recv()
    for x in payload_lines:
        ws.send(x)
        try:
            data = ws.recv()
            message = WebSocket_pb2.Message()
            message.ParseFromString(data)
            print(message)
        except websocket.WebSocketTimeoutException:
            continue
        time.sleep(pause)
    eprint("Done")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--url', type=str, help='Base URL for the database server', default='ws://localhost:3000/')
    parser.add_argument('identity', type=str, help='Identity hash')
    parser.add_argument('name', type=str, help='Module name')
    group = parser.add_mutually_exclusive_group()
    group.add_argument('--payload_file', type=str, help='A file of JSON payloads to send to the server', required=False)
    group.add_argument('--payload', type=str, help='JSON payload to send to the server', required=False)
    parser.add_argument('--pause', type=float, help="Pause time (in seconds) between message sends", default='0.100')

    ns = parser.parse_args()

    url = urllib.parse.urlparse(ns.url + "database/" + ns.identity + "/" + ns.name + "/subscribe")

    if ns.payload_file:
        with open(ns.payload_file, 'r') as p:
            payload_lines = p.read().splitlines()
    else:
        payload_lines = [ns.payload]

    ws_send(url, payload_lines, ns.pause)


if __name__ == "__main__":
    main()
