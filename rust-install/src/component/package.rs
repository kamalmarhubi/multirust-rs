extern crate tar;
extern crate flate2;

use component::components::*;
use component::transaction::*;

use errors::*;
use utils;
use temp;

use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::io::Read;
use std::fs::File;

pub trait Package {
	fn contains(&self, component: &str, short_name: Option<&str>) -> bool;
	fn install<'a>(&self, target: &Components, component: &str, short_name: Option<&str>, tx: Transaction<'a>) -> Result<Transaction<'a>>;
}

pub struct DirectoryPackage {
	path: PathBuf,
	components: HashSet<String>,
}

impl DirectoryPackage {
	pub fn new(path: PathBuf) -> Result<Self> {
		let content = try!(utils::read_file("package components", &path.join("components")));
		let components = content.lines().map(|l| l.to_owned()).collect();
		Ok(DirectoryPackage {
			path: path,
			components: components,
		})
	}
}

impl Package for DirectoryPackage {
	fn contains(&self, component: &str, short_name: Option<&str>) -> bool {
		self.components.contains(component) || if let Some(n) = short_name {
			self.components.contains(n)
		} else {
			false
		}
	}
	fn install<'a>(&self, target: &Components, name: &str, short_name: Option<&str>, tx: Transaction<'a>) -> Result<Transaction<'a>> {
		let actual_name = if self.components.contains(name) {
			name
		} else if let Some(n) = short_name {
			n
		} else {
			name
		};
		
		let root = self.path.join(actual_name);
		
		let manifest = try!(utils::read_file("package manifest", &root.join("manifest.in")));
		let mut builder = target.add(name, tx);
		
		for l in manifest.lines() {
			let part = try!(ComponentPart::decode(l).ok_or_else(|| {
				Error::CorruptComponent(name.to_owned())
			}));
			
			let path = part.1;
			let src_path = root.join(&path);
			
			match &*part.0 {
				"file" => try!(builder.copy_file(path, &src_path)),
				"dir" => try!(builder.copy_dir(path, &src_path)),
				_ => return Err(Error::CorruptComponent(name.to_owned())),
			}
		}
		
		let (_, tx) = try!(builder.finish());
		
		Ok(tx)
	}
}

pub struct TarPackage<'a>(DirectoryPackage, temp::Dir<'a>);

impl<'a> TarPackage<'a> {
	pub fn new<R: Read>(stream: R, temp_cfg: &'a temp::Cfg) -> Result<Self> {
		let temp_dir = try!(temp_cfg.new_directory());
		
		let mut archive = tar::Archive::new(stream);
		try!(archive.unpack(&*temp_dir).map_err(Error::ExtractingPackage));
		
		Ok(TarPackage(try!(DirectoryPackage::new(temp_dir.to_owned())), temp_dir))
	}
}

impl<'a> Package for TarPackage<'a> {
	fn contains(&self, component: &str, short_name: Option<&str>) -> bool {
		self.0.contains(component, short_name)
	}
	fn install<'b>(&self, target: &Components, component: &str, short_name: Option<&str>, tx: Transaction<'b>) -> Result<Transaction<'b>> {
		self.0.install(target, component, short_name, tx)
	}
}

pub struct TarGzPackage<'a>(TarPackage<'a>);

impl<'a> TarGzPackage<'a> {
	pub fn new<R: Read>(stream: R, temp_cfg: &'a temp::Cfg) -> Result<Self> {
		let stream = try!(flate2::read::GzDecoder::new(stream)
			.map_err(Error::ExtractingPackage));
		
		Ok(TarGzPackage(try!(TarPackage::new(stream, temp_cfg))))
	}
	pub fn new_file(path: &Path, temp_cfg: &'a temp::Cfg) -> Result<Self> {
		let file = try!(File::open(path).map_err(Error::ExtractingPackage));
		Self::new(file, temp_cfg)
	}
}

impl<'a> Package for TarGzPackage<'a> {
	fn contains(&self, component: &str, short_name: Option<&str>) -> bool {
		self.0.contains(component, short_name)
	}
	fn install<'b>(&self, target: &Components, component: &str, short_name: Option<&str>, tx: Transaction<'b>) -> Result<Transaction<'b>> {
		self.0.install(target, component, short_name, tx)
	}
}