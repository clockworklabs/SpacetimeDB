use std::{fs::{OpenOptions, File, read_dir}, path::{PathBuf, Path}, io::{BufWriter, Write, Read}, os::unix::prelude::{MetadataExt, FileExt}};

struct Segment {
    min_offset: u64,
    size: u64
}

impl Segment {
    fn name(&self) -> String {
        format!("{:0>20}", self.min_offset)
    }
}

struct MessageLog {
    root: PathBuf,
    segments: Vec<Segment>,
    open_segment_file: BufWriter<File>,
    open_segment_max_offset: u64,
    open_segment_size: u64,
}

impl MessageLog {
    fn open(path: &str) -> Result<Self, anyhow::Error> {
        let root = Path::new(path);

        let mut segments = Vec::new();
        for file in read_dir(root)? {
            let dir_entry = file?;
            let path = dir_entry.path();
            if path.extension().unwrap() == "log" {
                let file_stem = path.file_stem().unwrap();
                let offset = file_stem.to_os_string().into_string().unwrap();
                let offset = offset.parse::<u64>()?;
                let size = dir_entry.metadata()?.size();
                segments.push(Segment {
                    min_offset: offset,
                    size,
                });
            }
        }
        
        segments.sort_unstable_by_key(|s| s.min_offset);

        if segments.len() == 0 {
            segments.push(Segment { min_offset: 0, size: 0 });
        }

        let last_segment = segments.get(segments.len() - 1).unwrap();
        let last_segment_path = root.join(last_segment.name());
        let last_segment_size = last_segment.size;
        let file = OpenOptions::new().read(true).append(true).create(true).open(&last_segment_path)?;

        let mut max_offset = 0;
        let mut cursor: u64 = 0;
        while cursor < last_segment.size {
            let mut buf = [0; 8];
            file.read_exact_at(&mut buf, cursor)?;
            let message_len = u64::from_le_bytes(buf);

            max_offset += 1;
            cursor += 8 + message_len;
        }

        let file = BufWriter::new(file);

        Ok(Self { 
            root: root.to_owned(),
            segments,
            open_segment_file: file,
            open_segment_max_offset: max_offset,
            open_segment_size: last_segment_size,
        })
    }

    fn append(&mut self, message: impl AsRef<[u8]>) -> Result<(), anyhow::Error> {
        let message = message.as_ref();
        let size = message.len() as u64;

        let end_size = self.open_segment_size + size;
        if end_size > 1_000_000_000 {
            self.flush();
            self.segments.push(Segment {
                min_offset: self.open_segment_max_offset + 1,
                size: 0,
            });

            let last_segment = self.segments.get(self.segments.len() - 1).unwrap();
            let last_segment_path = self.root.join(last_segment.name());

            let file = OpenOptions::new().append(true).create(true).open(&last_segment_path)?;
            let file = BufWriter::new(file);
            
            self.open_segment_size = 0;
            self.open_segment_file = file;
        }

        self.open_segment_file.write_all(&size.to_le_bytes())?;
        self.open_segment_file.write_all(message)?;
        
        self.open_segment_size = end_size;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), anyhow::Error> {
        self.open_segment_file.flush()?;
        Ok(())
    }

    fn iter(&self) -> MessageLogIter {
        self.iter_from(0)
    }

    fn iter_from(&self, start_offset: u64) -> MessageLogIter {
        MessageLogIter { 
            offset: start_offset,
            message_log: self,
            open_segment_file: todo!(),
            open_segment_cursor: todo!(),
        }
    }
}

struct MessageLogIter<'a> {
    offset: u64,
    message_log: &'a MessageLog,
    open_segment_file: BufWriter<File>,
    open_segment_cursor: u64,
}

impl<'a> Iterator for MessageLogIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!()
    }
}



