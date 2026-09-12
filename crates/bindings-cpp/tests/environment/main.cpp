#include <spacetimedb/environment.h>
#include <spacetimedb/internal/v10_builder.h>
#include <spacetimedb/bsatn/writer.h>
#include <cassert>
#include <cstring>

using namespace SpacetimeDB;
std::string from_another_translation_unit();
namespace { std::string payload; size_t position; unsigned calls; }
extern "C" Status env_get(const uint8_t* key, uint32_t length, BytesSource* out) {
    const std::string name(reinterpret_cast<const char*>(key), length);
    ++calls;
    if (name == "LOG_LEVEL") { *out = BytesSource{0}; return Status{0}; }
    if (name == "FOOBAR") payload = calls == 1 ? "first" : "updated";
    else if (name == "ENABLE_EMAIL") payload = "false";
    else if (name == "DEPLOYMENT_KIND") payload = "production";
    else if (name == "get") payload = "reserved";
    else if (name == "class") payload = "keyword";
    else if (name == "NUL_LITERAL") payload = std::string("a\0b", 3);
    else return Status{1};
    position = 0;
    *out = BytesSource{1};
    return Status{0};
}
extern "C" int16_t bytes_source_read(BytesSource, uint8_t* out, size_t* size) {
    *size = std::min(*size, payload.size() - position);
    std::memcpy(out, payload.data() + position, *size);
    position += *size;
    return position == payload.size() ? -1 : 0;
}
extern "C" void console_log(LogLevel, const uint8_t*, size_t, const uint8_t*, size_t, uint32_t, const uint8_t*, size_t) {}

int main() {
    Environment env;
    static_assert(std::is_same_v<decltype(env.ENABLE_EMAIL()), std::string>);
    static_assert(std::is_same_v<decltype(env.LOG_LEVEL()), std::optional<std::string>>);
    assert(env.FOOBAR() == "first");
    assert(from_another_translation_unit() == "updated");
    assert(env.ENABLE_EMAIL() == "false");
    assert(!env.LOG_LEVEL());
    assert(env.DEPLOYMENT_KIND() == "production");
    assert(env.get("get") == "reserved");
    assert(env.get("class") == "keyword");
    assert(env.NUL_LITERAL() == std::string("a\0b", 3));
    const auto& entries = Internal::environment_declarations();
    assert(entries.size() == 7);
    assert(entries[0].name == "FOOBAR" && entries[0].constraint.get_tag() == 0 && !entries[0].optional);
    assert(entries[1].constraint.get_tag() == 2 && entries[1].constraint.get<2>() == std::vector<std::string>({"true", "false"}));
    assert(entries[2].optional);
    assert(entries[3].constraint.get_tag() == 1 && entries[3].constraint.get<1>() == "production");
    assert(entries[6].constraint.get<1>() == std::string("a\0b", 3));
    Internal::RawModuleDefV10Section section;
    section.set<15>(entries);
    assert(section.get_tag() == 15);
    assert(section.get<15>() == entries);
}
