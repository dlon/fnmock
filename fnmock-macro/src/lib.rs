use proc_macro::TokenStream;
use quote::quote;
use syn::{
    FnArg, ImplItem, ItemFn, ItemImpl, Type, parse::Parse, parse::ParseStream, parse_macro_input,
};

/// Enum that holds either a function or an impl block
enum MockableItem {
    Fn(ItemFn),
    Impl(ItemImpl),
}

impl Parse for MockableItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(syn::Token![impl]) {
            input.parse().map(MockableItem::Impl)
        } else {
            input.parse().map(MockableItem::Fn)
        }
    }
}

/// Makes a function or impl block mockable in tests.
///
/// # Functions
/// ```ignore
/// use fnmock::mockable;
///
/// #[mockable]
/// fn get_data() -> String {
///     "real data".to_string()
/// }
///
/// #[test]
/// fn test() {
///     let _guard = set_mock_get_data(|| "mocked".to_string());
///     assert_eq!(get_data(), "mocked");
/// }
/// ```
///
/// # Impl blocks
/// ```ignore
/// use fnmock::mockable;
///
/// struct MyService;
///
/// #[mockable]
/// impl MyService {
///     fn get_data(&self) -> String {
///         "real data".to_string()
///     }
/// }
///
/// #[test]
/// fn test() {
///     let service = MyService;
///     let _guard = MyService::set_mock_get_data(|_self| "mocked".to_string());
///     assert_eq!(service.get_data(), "mocked");
/// }
/// ```
#[proc_macro_attribute]
pub fn mockable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mockable_item = parse_macro_input!(item as MockableItem);

    match mockable_item {
        MockableItem::Fn(input_fn) => generate_mockable_fn(input_fn),
        MockableItem::Impl(input_impl) => generate_mockable_impl(input_impl),
    }
}

fn generate_mockable_fn(input_fn: ItemFn) -> TokenStream {
    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let original_fn_name = syn::Ident::new(&format!("__{}_original", fn_name), fn_name.span());

    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let block = &input_fn.block;
    let attrs = &input_fn.attrs;

    let is_async = sig.asyncness.is_some();

    let mut original_sig = sig.clone();
    original_sig.ident = original_fn_name.clone();

    // Extract parameter names and types for calling functions
    let param_names: Vec<_> = sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let syn::FnArg::Typed(pat_type) = arg {
                if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                    return Some(&pat_ident.ident);
                }
            }
            None
        })
        .collect();

    let param_types: Vec<&Type> = sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let syn::FnArg::Typed(pat_type) = arg {
                return Some(&*pat_type.ty);
            }
            None
        })
        .collect();

    let return_type = match &sig.output {
        syn::ReturnType::Default => quote! { () },
        syn::ReturnType::Type(_, ty) => quote! { #ty },
    };

    let mock_fn_type = if param_types.is_empty() {
        quote! { dyn Fn() -> #return_type + Send + Sync }
    } else {
        quote! { dyn Fn(#(#param_types),*) -> #return_type + Send + Sync }
    };

    let set_mock_helper = syn::Ident::new(&format!("set_mock_{}", fn_name), fn_name.span());

    let mock_key = quote! { concat!(module_path!(), "::", #fn_name_str) };

    let original_call = if is_async {
        quote! { #original_fn_name(#(#param_names),*).await }
    } else {
        quote! { #original_fn_name(#(#param_names),*) }
    };

    let expanded = quote! {
        // Original function - `cfg(not(test))` builds
        #(#attrs)*
        #[cfg(not(test))]
        #vis #sig #block

        // Rename original function for `cfg(test)`
        #[cfg(test)]
        #vis #original_sig #block

        // Helper function to set mocks with automatic type conversion
        #[cfg(test)]
        #[allow(dead_code)]
        pub fn #set_mock_helper<F>(mock: F) -> ::fnmock::MockGuard
        where
            F: Fn(#(#param_types),*) -> #return_type + Send + Sync + 'static,
        {
            let arc_mock: ::std::sync::Arc<#mock_fn_type> = ::std::sync::Arc::new(mock);
            ::fnmock::MockRegistry::set_mock(#mock_key, arc_mock)
        }

        // Wrapper function in test mode
        #[cfg(test)]
        #(#attrs)*
        #vis #sig {
            if let Some(mock_fn) = ::fnmock::MockRegistry::get_mock::<#mock_fn_type>(#mock_key) {
                return mock_fn(#(#param_names),*);
            }
            #original_call
        }
    };

    TokenStream::from(expanded)
}

/// Generate mockable code for an impl block
fn generate_mockable_impl(input_impl: ItemImpl) -> TokenStream {
    let self_ty = &input_impl.self_ty;
    let generics = &input_impl.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let type_name = quote!(#self_ty).to_string().replace(' ', "");

    let mut test_items = Vec::new();
    let mut non_test_items = Vec::new();
    let mut helper_functions = Vec::new();

    for item in &input_impl.items {
        if let ImplItem::Fn(method) = item {
            let fn_name = &method.sig.ident;
            let fn_name_str = fn_name.to_string();
            let original_fn_name =
                syn::Ident::new(&format!("__{}_original", fn_name), fn_name.span());

            let vis = &method.vis;
            let sig = &method.sig;
            let block = &method.block;
            let attrs = &method.attrs;

            let is_async = sig.asyncness.is_some();

            let mut original_sig = sig.clone();
            original_sig.ident = original_fn_name.clone();

            // Is there a self arg?
            let has_receiver = sig
                .inputs
                .first()
                .map_or(false, |arg| matches!(arg, FnArg::Receiver(_)));

            // Extract parameter names and types (excluding self)
            let param_names: Vec<_> = sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let FnArg::Typed(pat_type) = arg {
                        if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                            return Some(&pat_ident.ident);
                        }
                    }
                    None
                })
                .collect();

            let param_types: Vec<&Type> = sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let FnArg::Typed(pat_type) = arg {
                        return Some(&*pat_type.ty);
                    }
                    None
                })
                .collect();

            let return_type = match &sig.output {
                syn::ReturnType::Default => quote! { () },
                syn::ReturnType::Type(_, ty) => quote! { #ty },
            };

            // Generate mock key with type name included
            let mock_key = quote! { concat!(module_path!(), "::", #type_name, "::", #fn_name_str) };

            let set_mock_helper = syn::Ident::new(&format!("set_mock_{}", fn_name), fn_name.span());

            let (mock_fn_type, mock_call, helper_where) = if has_receiver {
                // For methods with self, the mock receives a reference to Self
                let receiver = sig.inputs.first().unwrap();
                let receiver_type = match receiver {
                    FnArg::Receiver(r) => {
                        if r.reference.is_some() {
                            if r.mutability.is_some() {
                                quote! { &mut #self_ty }
                            } else {
                                quote! { &#self_ty }
                            }
                        } else {
                            quote! { #self_ty }
                        }
                    }
                    _ => quote! { &#self_ty },
                };

                let fn_type = if param_types.is_empty() {
                    quote! { dyn Fn(#receiver_type) -> #return_type + Send + Sync }
                } else {
                    quote! { dyn Fn(#receiver_type, #(#param_types),*) -> #return_type + Send + Sync }
                };

                let call = quote! { mock_fn(self, #(#param_names),*) };

                let helper_where = quote! {
                    F: Fn(#receiver_type, #(#param_types),*) -> #return_type + Send + Sync + 'static
                };

                (fn_type, call, helper_where)
            } else {
                // Static method / associated function
                let fn_type = if param_types.is_empty() {
                    quote! { dyn Fn() -> #return_type + Send + Sync }
                } else {
                    quote! { dyn Fn(#(#param_types),*) -> #return_type + Send + Sync }
                };

                let call = quote! { mock_fn(#(#param_names),*) };

                let helper_where = quote! {
                    F: Fn(#(#param_types),*) -> #return_type + Send + Sync + 'static
                };

                (fn_type, call, helper_where)
            };

            // Call to the original function
            let original_call = match (has_receiver, is_async) {
                (true, true) => quote! { self.#original_fn_name(#(#param_names),*).await },
                (true, false) => quote! { self.#original_fn_name(#(#param_names),*) },
                (false, true) => quote! { Self::#original_fn_name(#(#param_names),*).await },
                (false, false) => quote! { Self::#original_fn_name(#(#param_names),*) },
            };

            // Non-test version (original)
            non_test_items.push(quote! {
                #(#attrs)*
                #vis #sig #block
            });

            // Test version - renamed original
            test_items.push(quote! {
                #(#attrs)*
                #vis #original_sig #block
            });

            // Test version - wrapper that checks for mock
            test_items.push(quote! {
                #(#attrs)*
                #vis #sig {
                    if let Some(mock_fn) = ::fnmock::MockRegistry::get_mock::<#mock_fn_type>(#mock_key) {
                        return #mock_call;
                    }
                    #original_call
                }
            });

            // Helper function for setting mock
            helper_functions.push(quote! {
                #[cfg(test)]
                #[allow(dead_code)]
                pub fn #set_mock_helper<F>(mock: F) -> ::fnmock::MockGuard
                where
                    #helper_where,
                {
                    let arc_mock: ::std::sync::Arc<#mock_fn_type> = ::std::sync::Arc::new(mock);
                    ::fnmock::MockRegistry::set_mock(#mock_key, arc_mock)
                }
            });
        } else {
            // Non-function items (consts, types, etc.) - keep as-is
            non_test_items.push(quote! { #item });
            test_items.push(quote! { #item });
        }
    }

    let expanded = quote! {
        #[cfg(not(test))]
        impl #impl_generics #self_ty #ty_generics #where_clause {
            #(#non_test_items)*
        }

        #[cfg(test)]
        impl #impl_generics #self_ty #ty_generics #where_clause {
            #(#test_items)*

            #(#helper_functions)*
        }
    };

    TokenStream::from(expanded)
}
