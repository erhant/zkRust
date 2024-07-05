use aligned_sdk::types::{ProvingSystemId, VerificationData};
use clap::{Args, Parser, Subcommand};
use ethers::prelude::{LocalWallet, Signer};
#[warn(unused_imports)]
use sp1_sdk::{ProverClient, SP1Stdin};
use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};
use std::sync::Arc;
use ethers::middleware::SignerMiddleware;
use ethers::providers::{Http, Provider};
use zkRust::pay_batcher;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Add flags that specify using proof system specific patches such as rand, sha, etc.

#[derive(Subcommand)]
enum Commands {
    /// Adds files to myapp
    ProveSp1(ProofArgs),
    ProveJolt(ProofArgs),
    ProveRisc0(ProofArgs),
}

#[derive(Args, Debug)]
struct ProofArgs {
    guest_path: String,
    output_proof_path: String,
    #[clap(long)]
    submit_to_aligned_with_keystore: Option<PathBuf>,
    #[clap(long)]
    std: bool,
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn add_text_after_substring(original_string: &str, substring: &str, text_to_add: &str) -> String {
    if let Some(index) = original_string.find(substring) {
        let mut modified_string = String::with_capacity(original_string.len() + text_to_add.len());
        modified_string.push_str(&original_string[..index + substring.len()]);
        modified_string.push_str(text_to_add);
        modified_string.push_str(&original_string[index + substring.len()..]);
        modified_string
    } else {
        original_string.to_string()
    }
}

fn prepend_to_file(file_path: &str, text_to_prepend: &str) -> io::Result<()> {
    // Open the file in read mode to read its existing content
    let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;

    // Read the existing content of the file
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    // Move the file cursor to the beginning of the file
    file.seek(io::SeekFrom::Start(0))?;

    // Write the text to prepend followed by the existing content back to the file
    file.write_all(text_to_prepend.as_bytes())?;
    file.write_all(content.as_bytes())?;
    file.flush()?;

    Ok(())
}

fn add_dependency_to_toml(path: &str, dep_string: &str) -> io::Result<()> {
    // Open the file in read write mode to read its existing content
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;

    // Read the existing content of the file
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    println!("Toml Content: {}", content);

    content = add_text_after_substring(&content, "[dependencies]", dep_string);

    println!("Toml Content after add: {}", content);

    file.set_len(0)?;
    file.seek(io::SeekFrom::Start(0))?;
    file.write_all(content.as_bytes())?;
    file.set_len(content.len() as u64)?;
    file.flush()?;

    Ok(())
}

fn replace_dependency_to_toml(path: &str, original_string: &str, replace_string: &str) -> io::Result<()> {
    // Open the file in read write mode to read its existing content
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;

    // Read the existing content of the file
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    //println!("Toml Content: {}", content);

    content = content.replace(original_string, replace_string);

    //println!("Toml Content after add: {}", content);

    file.set_len(0)?;
    file.seek(io::SeekFrom::Start(0))?;
    file.write_all(content.as_bytes())?;
    file.set_len(content.len() as u64)?;
    file.flush()?;

    Ok(())
}

// Tmp File directories
// TODO: create multiple of these for generating multiple proofs at once
const TMP_GUEST_DIR: &str = "./.tmp_guest";

const TMP_MAIN: &str = "./.tmp_guest/src/main.rs";

const TMP_CARGO_TOML: &str = "./.tmp_guest/Cargo.toml";

// SP1 File additions
const SP1_GUEST_DEPS_STRING: &str =
    "\nsp1-zkvm = { git = \"https://github.com/succinctlabs/sp1.git\" }\n";

const SP1_ELF_PATH: &str = "./.tmp_guest/elf/riscv32im-succinct-zkvm-elf";

const SP1_PROGRAM_HEADER: &str = "#![no_main]\nsp1_zkvm::entrypoint!(main);\n";

// Jolt File Additions
/*
const JOLT_TMP_GUEST_DIR: &str = "./.tmp_guest/guest";

const JOLT_TMP_MAIN: &str = "./.tmp_guest/guest/src/main.rs";

const JOLT_TMP_CARGO_TOML: &str = "./.tmp_guest/guest/Cargo.toml";

const JOLT_GUEST_TOML: &[u8] = b"[package]\nname = \"guest\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"guest\"\npath = \"./src/lib.rs\"\n\n[features]\nguest = []\n\n[dependencies]\njolt = { package = \"jolt-sdk\", git = \"https://github.com/a16z/jolt\", features = [\"guest-std\"] }";

const JOLT_GUEST_PROGRAM_HEADER: &str = "#![no_main]\n";

const JOLT_GUEST_FUNCTION_HEADER: &str = "#[jolt::provable]\n";

const JOLT_HOST_MAIN: &[u8] = b"use std::{io::Write, fs};\n\npub fn main() {\nlet (prove_fibonacci, verify_fibonacci) = guest::build_fibonacci();\n\nlet (program, _) = guest::preprocess_fibonacci();\n\n// Write elf to file outside of tmp directory\nlet elf = fs::read(program.elf.unwrap()).unwrap();\nlet mut file = fs::File::create(\"../guest.elf\").unwrap();\nfile.write_all(&elf).unwrap();\n\nlet (output, proof) = prove_fibonacci(50);\nproof.save_to_file(\"../guest.proof\").unwrap();\n\nlet is_valid = verify_fibonacci(proof);\n\nprintln!(\"output: {}\", output);\nprintln!(\"valid: {}\", is_valid);\n}";

const JOLT_HOST_CARGO: &[u8] = b"[package]\nname = \"method\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\nmembers = [\"guest\"]\n\n[profile.release]\ndebug = 1\ncodegen-units = 1\nlto = \"fat\"\n\n[dependencies]\njolt = { package = \"jolt-sdk\", git = \"https://github.com/a16z/jolt\", features = [\"host\"] }\nguest = { path = \"./guest\" }\n\n[patch.crates-io]\nark-ff = { git = \"https://github.com/a16z/arkworks-algebra\", branch = \"optimize/field-from-u64\" }\nark-ec = { git = \"https://github.com/a16z/arkworks-algebra\", branch = \"optimize/field-from-u64\" }\nark-serialize = { git = \"https://github.com/a16z/arkworks-algebra\", branch = \"optimize/field-from-u64\" }";

const JOLT_HOST_TOOLCHAIN: &[u8] =
    b"[toolchain]\nchannel = \"nightly-2024-04-20\"\ntargets = [\"riscv32i-unknown-none-elf\"]";
*/

// Risc 0 File Additions
const RISC0_DIR: &str = "./.risc_zero/";

const RISC0_GUEST_DIR: &str = "./.risc_zero/methods/guest/src";

const RISC0_GUEST_MAIN: &str = "./.risc_zero/methods/guest/src/main.rs";

const RISC0_GUEST_CARGO_TOML: &str = "./.risc_zero/methods/guest/Cargo.toml";

const RISC0_GUEST_PROGRAM_HEADER_STD: &str =
    "#![no_main]\n\nrisc0_zkvm::guest::entry!(main);\n";

const RISC0_GUEST_DEPS: &str =
    "\nrisc0-zkvm = { git = \"https://github.com/risc0/risc0\", features = [\"std\", \"getrandom\"] }";

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::ProveSp1(args) => {
            println!("'Proving with sp1 program in: {}", args.guest_path);
            // We create a temporary directory to edit the main.rs
            copy_dir_all(&args.guest_path, TMP_GUEST_DIR).unwrap();
            // Add needed file header
            /*
               #![no_main]
               sp1_zkvm::entrypoint!(main);
            */
            prepend_to_file(TMP_MAIN, SP1_PROGRAM_HEADER).unwrap();

            /*
            sp1-core = { git = "https://github.com/succinctlabs/sp1.git" }
             */
            add_dependency_to_toml(TMP_CARGO_TOML, SP1_GUEST_DEPS_STRING).unwrap();

            /*
                cd .tmp_guest
                cargo prove build
            */
            let guest_path = fs::canonicalize(TMP_GUEST_DIR).unwrap();
            Command::new("cargo")
                .arg("prove")
                .arg("build")
                .current_dir(guest_path)
                .output()
                .expect("Prove build failed");

            let elf_data = fs::read(&SP1_ELF_PATH).expect("unable to read metadata");

            // TODO: Write input to program.
            let stdin = SP1Stdin::new();

            let client = ProverClient::new();
            let (pk, vk) = client.setup(&elf_data);
            let proof = client.prove_compressed(&pk, stdin).expect("proving failed");


            // Verify proof.
            client
                .verify_compressed(&proof, &vk)
                .expect("verification failed");

            // Save proof.
            proof.save("sp1.proof").expect("saving proof failed");

            // Save elf
            let mut elf_file = fs::File::create("sp1.elf").expect("Failed to create sp1 elf file");
            elf_file
                .write_all(&elf_data)
                .expect("failed write sp1 elf to file");
            println!("generated proof");

            // Submit to aligned
            if let Some(keystore_path) = args.submit_to_aligned_with_keystore.clone() {
                let keystore_password = rpassword::prompt_password("Enter keystore password: ")
                    .expect("Failed to read keystore password");

                let wallet = LocalWallet::decrypt_keystore(keystore_path, &keystore_password)
                    .expect("Failed to decrypt keystore")
                    .with_chain_id(17000u64);

                let proof = bincode::serialize(&proof).expect("failed to serialize proof");

                let rpc_url = "https://ethereum-holesky-rpc.publicnode.com";

                let provider = Provider::<Http>::try_from(rpc_url)
                    .expect("Failed to connect to provider");

                let signer = Arc::new(SignerMiddleware::new(provider.clone(), wallet.clone()));

                let runtime = tokio::runtime::Runtime::new().expect("Failed to create runtime");

                runtime
                    .block_on(pay_batcher(wallet.address(), signer.clone()))
                    .expect("Failed to pay for proof submission");

                let verification_data = VerificationData {
                    proving_system: ProvingSystemId::SP1,
                    proof,
                    proof_generator_addr: wallet.address(),
                    vm_program_code: Some(elf_data),
                    verification_key: None,
                    pub_input: None,
                };

                println!("Submitting proof to aligned for verification");

                runtime
                    .block_on(zkRust::submit_proof_and_wait_for_verification(
                        verification_data,
                        wallet,
                        rpc_url.to_string(),
                    ))
                    .expect("failed to submit proof");

                println!("Proof submitted and verified on aligned");
            }
        }
        Commands::ProveJolt(_) => {
            todo!("Support Jolt once the Verifier is merged on Aligned");
        /*
            println!("'Proving with Jolt program in: {}", args.guest_path);
            /*
               Copy guest to guest directory structure
            */
            copy_dir_all(&args.guest_path, JOLT_TMP_GUEST_DIR).unwrap();

            /*
               #![cfg_attr(feature = \"guest\", no_std)]
               #![no_main]
            */
            prepend_to_file(JOLT_TMP_MAIN, JOLT_GUEST_PROGRAM_HEADER).unwrap();

            /*
               jolt = { package = "jolt-sdk", git = \"https://github.com/a16z/jolt\" }"
            */
            let mut guest_toml_file =
                fs::File::create(JOLT_TMP_CARGO_TOML).expect("could not open guest toml file");
            guest_toml_file
                .write_all(&JOLT_GUEST_TOML)
                .expect("failed to write guest toml");

            /*
               #[jolt::provable];
            */
            add_header_to_main(JOLT_TMP_MAIN, JOLT_GUEST_FUNCTION_HEADER).unwrap();
            fs::rename(JOLT_TMP_MAIN, "./.tmp_guest/guest/src/lib.rs").unwrap();

            // NOTE: Jolt only proves library functions and requires no main function within the library otherwise compilation to fail so we remove it using this hacky fix.
            let mut contents = String::new();
            File::open("./.tmp_guest/guest/src/lib.rs")
                .unwrap()
                .read_to_string(&mut contents)
                .unwrap();

            // Define a regular expression to match the main function
            // TODO: we need a more resilient way of doing this.
            let main_function_regex = Regex::new(r"(?s)pub fn main\(\) \{.*?\}").unwrap();

            // Remove the main function
            let modified_contents = main_function_regex.replace(&contents, "").to_string();

            // Write the modified contents back to the file
            let mut file = File::create("./.tmp_guest/guest/src/lib.rs").unwrap();
            file.write_all(modified_contents.as_bytes()).unwrap();
            //remove_text_from_file("./.tmp_guest/guest/src/lib.rs", &main_string).expect("failed to remvoe text");

            // to support std library compatibility we remove the blackbox
            remove_text_from_file("./.tmp_guest/guest/src/lib.rs", "use std::hint::black_box;")
                .expect("failed to remove text");

            /*
                create Host main.rs file

            */
            let src_dir = format!("{}/src", "./.tmp_guest/");
            fs::create_dir_all(&src_dir).expect("Failed to create src directory");
            let mut main_file =
                fs::File::create("./.tmp_guest/src/main.rs").expect("Failed to create lib.rs file");
            main_file
                .write_all(&JOLT_HOST_MAIN)
                .expect("Failed to write to main.rs file");

            /*
                create Host Cargo.toml
            */
            let mut toml_file = fs::File::create("./.tmp_guest/Cargo.toml")
                .expect("Failed to create Host Cargo.toml file");
            toml_file
                .write_all(&JOLT_HOST_CARGO)
                .expect("Failed to write to Host Cargo.toml file");

            /*
                create Host rust.toolchain
            */
            let mut toolchain_file = fs::File::create("./.tmp_guest/rust-toolchain.toml")
                .expect("Failed to create rust-toolchain.toml file");
            toolchain_file
                .write_all(JOLT_HOST_TOOLCHAIN)
                .expect("Failed to write to host rust-toolchain.toml file");

            let guest_path = fs::canonicalize(TMP_GUEST_DIR).unwrap();
            Command::new("cargo")
                .arg("run")
                .arg("--release")
                .current_dir(guest_path)
                .output()
                .expect("Prove build failed");
            println!("Elf and Proof generated!");
        */
        }
        Commands::ProveRisc0(args) => {
            println!("'Proving with Risc0 program in: {}", args.guest_path);
            // Clear contents of src directory
            fs::remove_dir_all(RISC0_GUEST_DIR).unwrap();
            fs::create_dir(RISC0_GUEST_DIR).unwrap();

            // Copy the source main to the destination directory
            let guest_path = format!("{}/src/", args.guest_path);
            copy_dir_all(&guest_path, &RISC0_GUEST_DIR).unwrap();

            // Copy dependencies to from guest toml to risc0 project template
            let toml_path = format!("{}/Cargo.toml", args.guest_path);
            let mut toml = std::fs::File::open(toml_path).unwrap();
            let mut content = String::new();
            toml.read_to_string(&mut content).unwrap();

            if let Some(start_index) = content.find("[dependencies]") {
                // Get all text after the search string
                let dependencies = &content[start_index + "[dependencies]".len()..];
                // Open the output file in append mode
                let mut guest_toml = OpenOptions::new()
                .create(true)
                .append(true)
                .open(RISC0_GUEST_CARGO_TOML).unwrap();
                println!("dependencies: {}", dependencies);

                // Write the text after the search string to the output file
                guest_toml.write_all(dependencies.as_bytes()).unwrap();
            } else {
                println!("Failed to copy dependencies in Guest Toml file, plese check");
            }

            /*
               #![no_main]
               risc0_zkvm::guest::entry!(main);
            */
            prepend_to_file(RISC0_GUEST_MAIN, RISC0_GUEST_PROGRAM_HEADER_STD).unwrap();

            let guest_path = fs::canonicalize(RISC0_DIR).unwrap();
            //TODO: propogate errors from this command to stdout/stderr
            Command::new("cargo")
                .arg("run")
                .arg("--release")
                .current_dir(guest_path)
                .output()
                .expect("Prove build failed");
            println!("Proof and Proof Image generated!");

            // Clear toml of dependencies
            let mut content = fs::read_to_string(RISC0_GUEST_CARGO_TOML).unwrap();
            if let Some(pos) = content.find("[dependencies]") {
                // Get all text after the search string
                content.truncate(pos + "[dependencies]".len());

                // Write the text after the search string to the output file
                let mut toml = fs::File::create(RISC0_GUEST_CARGO_TOML).unwrap();
                toml.write_all(content.as_bytes()).unwrap();
                // Append cargo toml dependency
                add_dependency_to_toml(RISC0_GUEST_CARGO_TOML, RISC0_GUEST_DEPS).unwrap();
            } else {
                println!("Failed to clear dependencies in Risc0 Toml file, plaese check");
            }
        }
    }
}
