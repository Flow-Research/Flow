use swiftide::indexing::{self, loaders::FileLoader};

pub struct Pipeline {}

impl Pipeline {
    pub fn start() {
        let pipeline = indexing::Pipeline::from_loader(FileLoader::new("./"));
    }
}
