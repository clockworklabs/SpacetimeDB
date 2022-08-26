# TODO(ryan): Implement TypeValue and tuples and all that jazz. The hard stuff.

# Called when an identity connects or disconnects.
def __identity_connected__(identity, timestamp):
    SpacetimeDB.console_log(5, "identity connected: %s", identity)


def __identity_disconnected__(identity, timestamp):
    SpacetimeDB.console_log(5, "identity disconnected: %s", identity)


# Hook for table creation. Doesn't actually create a table yet.
def __create_table__mock():
    SpacetimeDB.console_log(1, "create_table__mock called")


# An example reducer which merely calls console_log for now.
def __reducer__test(identity, timestamp, args):
    identity_hex = ''.join('{:02x}'.format(x) for x in identity)
    SpacetimeDB.console_log(1, "'test' call from identity: %s, timestamp: %s, arguments: %s"
                            % ( identity_hex, timestamp, args))
