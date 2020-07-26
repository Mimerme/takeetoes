use log::{info, debug};
use sha2::{Sha256, Sha512, Digest};
use std::path::{Path, PathBuf};
use std::fs::{metadata, File};
use std::collections::{HashSet, HashMap, VecDeque};
use std::fs::{read_to_string, write};

pub fn get_directory_hash(project_dir : &str, files : &mut HashMap<PathBuf, (PathBuf, String)>, output_files : bool) -> (Vec<u8>, Vec<u8>){
     let mut structure_hasher = Sha512::new();
     let mut content_hasher = Sha512::new();

     //Load/Create the .gitignore file
     let ignore = format!("{}/.gitignore", project_dir.clone());
     let gitignore_path = Path::new(&ignore);

     if !Path::exists(gitignore_path){
        File::create(gitignore_path); 
     }
 
     let git_ignore = gitignore::File::new(gitignore_path).unwrap();
     let project_dir = &project_dir.clone();

     debug!("project_dir = {}, gitignore = {:?}", project_dir, gitignore_path);

     //Get all the files that are not excluded by the .gitignore and sort them alphabetically
     let mut proj_iter = git_ignore.included_files().unwrap();
     proj_iter.sort_by(|a, b| a.file_name().unwrap().cmp(b.file_name().unwrap()));


     for entry in proj_iter.iter(){
         //Remove the beginning /home/user/... stuff so project structures are the same across machines
         //
         //host_path refers to the local machine's path to the file
         //net_path refers to the p2p network's identifier for the file
         let host_path = entry.as_path().clone();
         let net_path = host_path.strip_prefix(Path::new(project_dir)).expect("Error stripping the prefix of a project entry");
         debug!("New Network Entry: {:?}", &net_path);

         //Update the project structure hash
         structure_hasher.update(net_path.to_str().unwrap().as_bytes());

         //TODO: clean up this block
         if metadata(host_path.clone()).unwrap().is_file() {
            let file_contents = read_to_string(host_path.clone()).unwrap();
            content_hasher.update(file_contents.as_bytes());

            if output_files{
                files.insert(PathBuf::from(net_path.to_str().unwrap().to_string().replace("\\", "/")), 
                    (PathBuf::from(host_path.to_str().unwrap().to_string().replace("\\", "/")), file_contents));
            }
         }

     }

     //finalize the hashes and return them
     let structure_hash = structure_hasher.finalize();
     let content_hash = content_hasher.finalize();
     return (structure_hash.to_vec(), content_hash.to_vec());
}
