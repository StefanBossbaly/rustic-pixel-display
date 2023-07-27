extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, FieldsUnnamed, GenericParam, TypeParamBound,
};

#[proc_macro_derive(RenderFactories)]
pub fn derive_configurable(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let name = &ast.ident;

    let (name_variants, description_variants, load_variants) = match &ast.data {
        Data::Enum(enum_data) => {
            let mut enum_name = Vec::new();
            let mut enum_description = Vec::new();
            let mut enum_load_from_config = Vec::new();

            enum_data.variants.iter().for_each(|variant| {
                let variant_name = &variant.ident;

                match &variant.fields {
                    Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
                        if unnamed.len() != 1 {
                            panic!("derive(RenderFactory) only supports enums");
                        }

                        let render_name = quote! {
                            Self::#variant_name(__self) => {
                                __self.render_name()
                            }
                        };

                        let render_description = quote! {
                            Self::#variant_name(__self) => {
                                __self.render_description()
                            }
                        };

                        let render_load_from_config = quote! {
                            Self::#variant_name(__self) => {
                                __self.load_from_config(reader)
                            }
                        };

                        enum_name.push(render_name);
                        enum_description.push(render_description);
                        enum_load_from_config.push(render_load_from_config);
                    }
                    Fields::Named(_) | Fields::Unit => {
                        panic!("derive(RenderFactory) only supports enums");
                    }
                }
            });

            (enum_name, enum_description, enum_load_from_config)
        }
        _ => panic!("derive(RenderFactory) only supports enums"),
    };

    let draw_target_ident = ast
        .generics
        .params
        .iter()
        .find_map(|param| {
            if let GenericParam::Type(type_param) = param {
                type_param.bounds.iter().find_map(|bound| {
                    if let TypeParamBound::Trait(trait_bound) = bound {
                        if let Some(last_segment) = trait_bound.path.segments.last() {
                            if last_segment.ident == "DrawTarget" {
                                return Some(type_param.ident.clone());
                            }
                        }
                    }

                    None
                })
            } else {
                None
            }
        })
        .expect("Could not draw DrawTarget bound in Enum");

    let (impl_generics, type_generics, where_clause) = ast.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics rustic_pixel_display::render::RenderFactory<#draw_target_ident> for #name #type_generics #where_clause {
            fn render_name(&self) -> &'static str {
                match self {
                    #(#name_variants)*
                }
            }

            fn render_description(&self) -> &'static str {
                match self {
                    #(#description_variants)*
                }
            }

            fn load_from_config<R: std::io::Read>(&self, reader: R) -> anyhow::Result<Box<dyn Render<D>>> {
                match self {
                    #(#load_variants)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
