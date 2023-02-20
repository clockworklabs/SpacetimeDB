pub mod odb {
    use crate::hash::hash_bytes;
    use crate::hash::Hash;
    use hex;
    use std::path::PathBuf;
    use tokio::fs;

    const ROOT: &str = "/stdb/odb";

    pub async fn total_key_size_bytes() -> u64 {
        unimplemented!()
    }

    pub async fn total_obj_size_bytes() -> u64 {
        unimplemented!()
    }

    pub async fn total_mem_size_bytes() -> u64 {
        total_key_size_bytes().await + total_obj_size_bytes().await
    }

    pub async fn add(bytes: impl AsRef<[u8]>) -> Hash {
        let hash = hash_bytes(&bytes);

        let folder = hex::encode(&hash[0..2]);
        let filename = hex::encode(&hash[2..]);
        let path = PathBuf::from(format!("{}/{}/{}", ROOT, folder, filename));

        if let Some(p) = path.parent() {
            fs::create_dir_all(p).await.unwrap()
        }
        fs::write(path, bytes).await.unwrap();

        hash
    }

    pub async fn get(hash: Hash) -> Option<Vec<u8>> {
        let folder = hex::encode(&hash[0..2]);
        let filename = hex::encode(&hash[2..]);
        let path = PathBuf::from(format!("{}/{}/{}", ROOT, folder, filename));

        if !path.exists() {
            return None;
        }

        Some(fs::read(path).await.unwrap())
    }
}
