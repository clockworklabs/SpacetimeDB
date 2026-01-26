struct RawModuleDefV10 {
    sections: Vec<RawModuleDefV10Section>,
}

enum RawModuleDefV10Section {
    Types,
    Tables,
    Reducers,
    Procedures,
    Views,
    Schedules,
    Events,
}
