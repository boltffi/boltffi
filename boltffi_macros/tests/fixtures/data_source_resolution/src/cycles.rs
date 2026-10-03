#[boltffi::data]
#[derive(Clone)]
pub struct Folder {
    pub name: String,
    pub files: Vec<File>,
}

#[boltffi::data]
#[derive(Clone)]
pub struct File {
    pub name: String,
    pub subfolders: Vec<Folder>,
}

#[boltffi::data(impl)]
impl Folder {
    pub fn first(&self) -> Option<File> {
        self.files.first().cloned()
    }
}

pub mod path {
    #[boltffi::data]
    #[derive(Clone)]
    pub struct Trail {
        pub name: String,
        pub next: Vec<crate::cycles::path::Trail>,
    }
}

pub struct Library {
    pub shelves: u32,
}

pub struct Book {
    pub shelf: u32,
}

#[boltffi::export]
impl Library {
    pub fn new() -> Library {
        Library { shelves: 1 }
    }

    pub fn book(&self) -> Book {
        Book {
            shelf: self.shelves,
        }
    }
}

#[boltffi::export]
impl Book {
    pub fn library(&self) -> Library {
        Library {
            shelves: self.shelf,
        }
    }
}
