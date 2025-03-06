use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenTree};
use quote::{quote, ToTokens};
use regex::Regex;
use syn::{
	parse2, parse_macro_input, parse_str, punctuated::Punctuated, token::Comma, Expr, FnArg, ItemFn, ReturnType, Type
};

#[proc_macro_attribute]
pub fn tauri_command(_: TokenStream, item: TokenStream) -> TokenStream {
	let function = parse_macro_input!(item as ItemFn);

	let func_name = &function.sig.ident;
	let func_args = &function.sig.inputs;
	let func_ret = &function.sig.output;

	// Add rs_ to generated function name
	let gen_name = Ident::new(&format!("rs_{}", func_name), func_name.span());

	// Remove all & references from function args for generated function
	let mut gen_args: Punctuated<FnArg, Comma> = Punctuated::new();
	for arg in func_args.iter() {
		if let FnArg::Typed(arg) = arg.to_owned() {
			gen_args.push(FnArg::Typed(syn::PatType {
				attrs: arg.attrs,
				pat: arg.pat,
				colon_token: arg.colon_token,
				ty: Box::new(
					parse2(
						arg.ty
							.to_token_stream()
							.into_iter()
							.filter(|x: &TokenTree| !matches!(x, TokenTree::Punct(x) if x.to_string() == "&"))
							.filter(|x: &TokenTree| !matches!(x, TokenTree::Ident(x) if x.to_string() == "mut"))
							.collect::<proc_macro2::TokenStream>()
					)
					.unwrap()
				)
			}));
		}
	}

	// Reframe result type (Result<XYZ, String>) for generated function
	let gen_ret = match func_ret {
		ReturnType::Default => unimplemented!("Macro can only be used on fallible methods"),
		ReturnType::Type(arrow, ty) => {
			let ok_type: Type = parse_str(
				&Regex::new("Result<(.*)>")
					.unwrap()
					.replace(&ty.to_token_stream().to_string().replace(' ', ""), "$1")
			)
			.unwrap();

			ReturnType::Type(
				arrow.to_owned(),
				Box::new(parse2(quote!(Result<#ok_type, String>)).unwrap())
			)
		}
	};

	// Recreate arguments used to call original function (including references this time to keep consistency with input function's signature)
	let mut call_args: Punctuated<Expr, Comma> = Punctuated::new();
	call_args.extend(func_args.iter().map(|x| {
		match x {
			FnArg::Receiver(_) => unimplemented!("Macro can't be used on methods"),
			FnArg::Typed(arg) => parse2::<Expr>(
				arg.ty
					.to_token_stream()
					.into_iter()
					.filter(|x: &TokenTree| {
						matches!(x, TokenTree::Punct(x) if x.to_string() == "&")
							|| matches!(x, TokenTree::Ident(x) if x.to_string() == "mut")
					})
					.chain(arg.pat.to_token_stream().into_iter())
					.collect()
			)
			.unwrap()
		}
	}));

	// Return tokens representing generated function and original function
	quote! {
		#[tauri::command]
		#[specta::specta]
		pub fn #gen_name(#gen_args) #gen_ret {
			#func_name(#call_args).map_err(|x| format!("{x:?}"))
		}

		#function
	}
	.into()
}

#[proc_macro_attribute]
pub fn async_tauri_command(_: TokenStream, item: TokenStream) -> TokenStream {
	let function = parse_macro_input!(item as ItemFn);

	let func_name = &function.sig.ident;
	let func_args = &function.sig.inputs;
	let func_ret = &function.sig.output;

	// Add rs_ to generated function name
	let gen_name = Ident::new(&format!("rs_{}", func_name), func_name.span());

	// Remove all & references from function args for generated function
	let mut gen_args: Punctuated<FnArg, Comma> = Punctuated::new();
	for arg in func_args.iter() {
		if let FnArg::Typed(arg) = arg.to_owned() {
			gen_args.push(FnArg::Typed(syn::PatType {
				attrs: arg.attrs,
				pat: arg.pat,
				colon_token: arg.colon_token,
				ty: Box::new(
					parse2(
						arg.ty
							.to_token_stream()
							.into_iter()
							.filter(|x: &TokenTree| !matches!(x, TokenTree::Punct(x) if x.to_string() == "&"))
							.filter(|x: &TokenTree| !matches!(x, TokenTree::Ident(x) if x.to_string() == "mut"))
							.collect::<proc_macro2::TokenStream>()
					)
					.unwrap()
				)
			}));
		}
	}

	// Reframe result type (Result<XYZ, String>) for generated function
	let gen_ret = match func_ret {
		ReturnType::Default => unimplemented!("Macro can only be used on fallible methods"),
		ReturnType::Type(arrow, ty) => {
			let ok_type: Type = parse_str(
				&Regex::new("Result<(.*)>")
					.unwrap()
					.replace(&ty.to_token_stream().to_string().replace(' ', ""), "$1")
			)
			.unwrap();

			ReturnType::Type(
				arrow.to_owned(),
				Box::new(parse2(quote!(Result<#ok_type, String>)).unwrap())
			)
		}
	};

	// Recreate arguments used to call original function (including references this time to keep consistency with input function's signature)
	let mut call_args: Punctuated<Expr, Comma> = Punctuated::new();
	call_args.extend(func_args.iter().map(|x| {
		match x {
			FnArg::Receiver(_) => unimplemented!("Macro can't be used on methods"),
			FnArg::Typed(arg) => parse2::<Expr>(
				arg.ty
					.to_token_stream()
					.into_iter()
					.filter(|x: &TokenTree| {
						matches!(x, TokenTree::Punct(x) if x.to_string() == "&")
							|| matches!(x, TokenTree::Ident(x) if x.to_string() == "mut")
					})
					.chain(arg.pat.to_token_stream().into_iter())
					.collect()
			)
			.unwrap()
		}
	}));

	// Return tokens representing generated function and original function
	quote! {
		#[tauri::command]
		#[specta::specta]
		pub async fn #gen_name(#gen_args) #gen_ret {
			#func_name(#call_args).await.map_err(|x| format!("{x:?}"))
		}

		#function
	}
	.into()
}
