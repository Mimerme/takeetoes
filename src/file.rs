
use sha2::{Sha256, Sha512, Digest};

//Fuinctions for file IO operations found here

//Returns the structure and content hash
//2 seperate hashes, one represents the directory structure, the other the file contents
fn get_directory_hash(project_dir : String, files :&mut HashMap<String, (String, String)>, output_files : bool) -> (Vec<u8>, Vec<u8>){
     let mut proj_hasher = Sha512::new();
     let mut file_hasher = Sha512::new();

     //Load/Create the .gitignore file
     let ignore = format!("{}/.gitignore", project_dir.clone());
     let gitignore_path = Path::new(&ignore);

     if !Path::exists(gitignore_path){
        File::create(gitignore_path); 
     }
        
     let git_ignore = gitignore::File::new(gitignore_path).unwrap();
     let project_dir = &project_dir.clone();

     println!("Project Directory: {:?}", project_dir);
     println!("Loading the .gitignore file from: {:?}", gitignore_path);

     //Get all the files that are not excluded by the .gitignore
     let mut proj_iter = git_ignore.included_files().unwrap();
     proj_iter.sort_by(|a, b| a.file_name().unwrap().cmp(b.file_name().unwrap()));

    println!("{:?}", proj_iter);

     //for entry in WalkDir::new(&project_dir).sort_by(|a,b| a.file_name().cmp(b.file_name())) {
     for entry in proj_iter.iter(){
         //Remove the beginning /home/user/... stuff so project structures are the same across machines
         //
         //host_path refers to the local machine's path to the file
         //net_path refers to the p2p network's identifier for the file
         let host_path = entry.as_path().clone();
         let net_path = host_path.strip_prefix(Path::new(project_dir)).expect("Error stripping the prefix of a project entry");
         println!("Potential Network Entry: {:?}", &net_path);

         //Update the project structure hash
         proj_hasher.update(net_path.to_str().unwrap().as_bytes());

         if metadata(host_path.clone()).unwrap().is_file() {
            let file_contents = read_to_string(host_path.clone()).unwrap();
            file_hasher.update(file_contents.as_bytes());

            if output_files{
                files.insert(net_path.to_str().unwrap().to_string(), (host_path.to_str().unwrap().to_string(), file_contents));
            }
         }

     }
     //Generate the hashes and return them
     let proj_hash = proj_hasher.finalize();
     let file_hash = file_hasher.finalize();
     return (proj_hash.to_vec(), file_hash.to_vec());
}
