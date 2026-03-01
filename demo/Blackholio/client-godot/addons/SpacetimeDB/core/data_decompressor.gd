class_name DataDecompressor extends RefCounted

static func decompress_packet(compressed_bytes: PackedByteArray) -> PackedByteArray:
    if compressed_bytes.is_empty():
        return PackedByteArray()

    var gzip_stream := StreamPeerGZIP.new()
    
    if gzip_stream.start_decompression() != OK:
        printerr("DataDecompressor Error: Failed to start Gzip decompression.")
        return []
        
    if gzip_stream.put_data(compressed_bytes) != OK:
        printerr("DataDecompressor Error: Failed to put data into Gzip stream.")
        return []
        
    var decompressed_data := PackedByteArray()
    var chunk_size := 4096 
    
    while true:
        var result: Array = gzip_stream.get_partial_data(chunk_size)
        var status: Error = result[0]
        var chunk: PackedByteArray = result[1]

        if status == OK:
            if chunk.is_empty():
                break
            decompressed_data.append_array(chunk)
        elif status == ERR_UNAVAILABLE:
            break
        else:
            printerr("DataDecompressor Error: Failed while getting partial data.")
            return []
    return decompressed_data
