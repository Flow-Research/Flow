use instant_distance::{Builder, HnswMap, Search};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VectorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Index not initialized")]
    NotInitialized,
    #[error("Vector dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("CID not found: {0}")]
    CidNotFound(String),
}

#[derive(Clone, Serialize, Deserialize)]
struct VectorPoint {
    embedding: Vec<f32>,
    cid: String,
}

impl instant_distance::Point for VectorPoint {
    fn distance(&self, other: &Self) -> f32 {
        cosine_distance(&self.embedding, &other.embedding)
    }
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0;
    }

    1.0 - (dot / (norm_a * norm_b))
}

#[derive(Serialize, Deserialize)]
struct IndexData {
    points: Vec<VectorPoint>,
    cid_to_idx: HashMap<String, usize>,
}

pub struct VectorIndex {
    index: RwLock<Option<HnswMap<VectorPoint, usize>>>,
    points: RwLock<Vec<VectorPoint>>,
    cid_to_idx: RwLock<HashMap<String, usize>>,
    index_path: PathBuf,
    dimensions: usize,
    dirty: RwLock<bool>,
}

impl VectorIndex {
    pub fn new(path: &Path, dimensions: usize) -> Self {
        Self {
            index: RwLock::new(None),
            points: RwLock::new(Vec::new()),
            cid_to_idx: RwLock::new(HashMap::new()),
            index_path: path.to_owned(),
            dimensions,
            dirty: RwLock::new(false),
        }
    }

    pub fn load(path: &Path, dimensions: usize) -> Result<Self, VectorError> {
        let index_file = path.join("vector_index.bin");
        let data_file = path.join("vector_data.bin");

        if !data_file.exists() {
            return Ok(Self::new(path, dimensions));
        }

        let data_bytes = fs::read(&data_file)?;
        let data: IndexData = bincode::deserialize(&data_bytes)
            .map_err(|e| VectorError::Serialization(e.to_string()))?;

        let instance = Self {
            index: RwLock::new(None),
            points: RwLock::new(data.points),
            cid_to_idx: RwLock::new(data.cid_to_idx),
            index_path: path.to_owned(),
            dimensions,
            dirty: RwLock::new(false),
        };

        if index_file.exists() {
            instance.rebuild_index()?;
        }

        Ok(instance)
    }

    pub fn add(&self, cid: &str, embedding: &[f32]) -> Result<(), VectorError> {
        if embedding.len() != self.dimensions {
            return Err(VectorError::DimensionMismatch {
                expected: self.dimensions,
                actual: embedding.len(),
            });
        }

        let point = VectorPoint {
            embedding: embedding.to_vec(),
            cid: cid.to_string(),
        };

        let mut points = self.points.write().unwrap();
        let mut cid_to_idx = self.cid_to_idx.write().unwrap();

        if cid_to_idx.contains_key(cid) {
            return Ok(());
        }

        let idx = points.len();
        points.push(point);
        cid_to_idx.insert(cid.to_string(), idx);

        *self.dirty.write().unwrap() = true;

        if points.len() % 1000 == 0 {
            drop(points);
            drop(cid_to_idx);
            self.rebuild_and_save()?;
        }

        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>, VectorError> {
        if query.len() != self.dimensions {
            return Err(VectorError::DimensionMismatch {
                expected: self.dimensions,
                actual: query.len(),
            });
        }

        let index_guard = self.index.read().unwrap();
        let index = match index_guard.as_ref() {
            Some(idx) => idx,
            None => {
                drop(index_guard);
                self.rebuild_index()?;
                let guard = self.index.read().unwrap();
                if guard.is_none() {
                    return Ok(Vec::new());
                }
                return self.search(query, k);
            }
        };

        let query_point = VectorPoint {
            embedding: query.to_vec(),
            cid: String::new(),
        };

        let mut search = Search::default();
        let results: Vec<_> = index.search(&query_point, &mut search).take(k).collect();

        let points = self.points.read().unwrap();
        Ok(results
            .into_iter()
            .filter_map(|item| {
                let idx = *item.value;
                points.get(idx).map(|p| (p.cid.clone(), item.distance))
            })
            .collect())
    }

    pub fn contains(&self, cid: &str) -> bool {
        self.cid_to_idx.read().unwrap().contains_key(cid)
    }

    pub fn len(&self) -> usize {
        self.points.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn save(&self) -> Result<(), VectorError> {
        if !*self.dirty.read().unwrap() {
            return Ok(());
        }

        fs::create_dir_all(&self.index_path)?;

        let points = self.points.read().unwrap();
        let cid_to_idx = self.cid_to_idx.read().unwrap();

        let data = IndexData {
            points: points.clone(),
            cid_to_idx: cid_to_idx.clone(),
        };

        let data_bytes =
            bincode::serialize(&data).map_err(|e| VectorError::Serialization(e.to_string()))?;

        let data_file = self.index_path.join("vector_data.bin");
        fs::write(&data_file, data_bytes)?;

        *self.dirty.write().unwrap() = false;

        Ok(())
    }

    fn rebuild_index(&self) -> Result<(), VectorError> {
        let points = self.points.read().unwrap();

        if points.is_empty() {
            *self.index.write().unwrap() = None;
            return Ok(());
        }

        let values: Vec<usize> = (0..points.len()).collect();
        let hnsw = Builder::default().build(points.clone(), values);

        *self.index.write().unwrap() = Some(hnsw);

        Ok(())
    }

    fn rebuild_and_save(&self) -> Result<(), VectorError> {
        self.rebuild_index()?;
        self.save()?;
        Ok(())
    }

    pub fn flush(&self) -> Result<(), VectorError> {
        self.rebuild_and_save()
    }
}

impl Drop for VectorIndex {
    fn drop(&mut self) {
        if let Err(e) = self.save() {
            tracing::error!("Failed to save vector index on drop: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn random_embedding(dim: usize) -> Vec<f32> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..dim).map(|_| rng.r#gen::<f32>()).collect()
    }

    fn normalize(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    #[test]
    fn test_add_and_search() {
        let temp_dir = TempDir::new().unwrap();
        let index = VectorIndex::new(temp_dir.path(), 768);

        for i in 0..100 {
            let mut emb = random_embedding(768);
            normalize(&mut emb);
            index.add(&format!("cid_{}", i), &emb).unwrap();
        }

        index.rebuild_index().unwrap();

        let mut query = random_embedding(768);
        normalize(&mut query);
        let results = index.search(&query, 10).unwrap();

        assert_eq!(results.len(), 10);
    }

    #[test]
    fn test_dimension_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let index = VectorIndex::new(temp_dir.path(), 768);

        let wrong_dim = vec![0.1; 512];
        let result = index.add("test", &wrong_dim);

        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();

        {
            let index = VectorIndex::new(temp_dir.path(), 768);
            for i in 0..50 {
                let mut emb = random_embedding(768);
                normalize(&mut emb);
                index.add(&format!("cid_{}", i), &emb).unwrap();
            }
            index.flush().unwrap();
        }

        let loaded = VectorIndex::load(temp_dir.path(), 768).unwrap();
        assert_eq!(loaded.len(), 50);
        assert!(loaded.contains("cid_0"));
        assert!(loaded.contains("cid_49"));
    }

    #[test]
    fn test_cosine_distance() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_distance(&a, &b) - 0.0).abs() < 0.0001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_distance(&a, &c) - 1.0).abs() < 0.0001);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_distance(&a, &d) - 2.0).abs() < 0.0001);
    }
}
