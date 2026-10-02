pub(super) trait SectionPayload {
    fn skip_serializing(&self) -> bool {
        false
    }
}

impl<T> SectionPayload for Vec<T> {
    fn skip_serializing(&self) -> bool {
        self.is_empty()
    }
}

impl SectionPayload for spacetimedb_sats::Typespace {
    fn skip_serializing(&self) -> bool {
        self.types.is_empty()
    }
}

macro_rules! define_section_types {
    (version = $VN:ident, $($(#[$attr:meta])* $name:ident($payload:ty),)*) => {
        paste::paste! {
            #[doc = "A section of a " $VN " module definition."]
            #[derive(Debug, Clone, SpacetimeType)]
            #[sats(crate = crate)]
            #[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
            #[non_exhaustive]
            pub enum [< RawModuleDef $VN Section >] {
                $( $(#[$attr])* $name($payload), )*
            }

            #[derive(Debug, Default)]
            pub struct [< RawModuleDef $VN Sections >] {
                $($(#[$attr])* pub [< $name:snake >]: Option<$payload>,)*
            }

            impl FromIterator<[< RawModuleDef $VN Section >]> for [< RawModuleDef $VN Sections >] {
                fn from_iter<I: IntoIterator<Item = [< RawModuleDef $VN Section >]>>(iter: I) -> Self {
                    let mut sections = Self::default();
                    for section in iter {
                        match section {
                            // TODO(noa): should we error when coming across duplicate sections? merge them?
                            $([< RawModuleDef $VN Section >]::$name(payload) => { sections.[< $name:snake >].get_or_insert(payload); })*
                        }
                    }
                    sections
                }
            }

            impl [< RawModuleDef $VN Sections >] {
                #[allow(path_statements)]
                const NUM_SECTIONS: usize = [$({ [< RawModuleDef $VN Section >]::$name; }),*].len();

                $(pub fn [< $name:snake _mut >](&mut self) -> &mut $payload {
                    self.[< $name:snake >].get_or_insert_default()
                })*
            }

            impl IntoIterator for [< RawModuleDef $VN Sections >] {
                type Item = [< RawModuleDef $VN Section >];
                type IntoIter = std::iter::Flatten<std::array::IntoIter<Option<[< RawModuleDef $VN Section >]>, { Self::NUM_SECTIONS }>>;
                fn into_iter(self) -> Self::IntoIter {
                    [
                        $(self.[< $name:snake >]
                            .filter(|x| !$crate::db::raw_def::sections::SectionPayload::skip_serializing(x))
                            .map([< RawModuleDef $VN Section >]::$name),)*
                    ]
                    .into_iter()
                    .flatten()
                }
            }
        }
    };
}
pub(super) use define_section_types;
