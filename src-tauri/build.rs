fn main() {
	#[cfg(windows)]
	{
		println!("cargo:rustc-link-lib=crypt32");
		println!("cargo:rustc-link-lib=Setupapi");
		println!("cargo:rustc-link-lib=winmm");
		println!("cargo:rustc-link-lib=Imm32");
		println!("cargo:rustc-link-lib=Version");
		println!("cargo:rustc-link-lib=mfplat");
		println!("cargo:rustc-link-lib=strmiids");
		println!("cargo:rustc-link-lib=mfuuid");
		println!("cargo:rustc-link-lib=Vfw32");
	}

	tauri_build::build()
}
