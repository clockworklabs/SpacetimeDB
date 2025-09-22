#ifndef SPACETIMEDB_BSATN_SCHEDULE_AT_IMPL_H
#define SPACETIMEDB_BSATN_SCHEDULE_AT_IMPL_H

#include "schedule_at.h"
#include "reader.h"
#include "writer.h"
#include "traits.h"
#include "type_extensions.h"  // For special type constants

namespace SpacetimeDb {

// ScheduleAt BSATN implementation
inline void ScheduleAt::bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const {
    // Write variant tag (as u8)
    writer.write_u8(static_cast<uint8_t>(variant));
    
    // Write payload based on variant
    switch (variant) {
        case Variant::Interval:
            interval_value.bsatn_serialize(writer);
            break;
        case Variant::Time:
            time_value.bsatn_serialize(writer);
            break;
    }
}

inline void ScheduleAt::bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader) {
    // Read variant tag
    uint8_t variant_tag = reader.read_u8();
    
    // Destroy current union content
    this->~ScheduleAt();
    
    // Initialize based on variant
    switch (variant_tag) {
        case 0: // Interval
            variant = Variant::Interval;
            new (&interval_value) TimeDuration();
            interval_value.bsatn_deserialize(reader);
            break;
        case 1: // Time
            variant = Variant::Time;
            new (&time_value) Timestamp();
            time_value.bsatn_deserialize(reader);
            break;
        default:
            // Default to Interval variant with 0 duration
            variant = Variant::Interval;
            new (&interval_value) TimeDuration();
    }
}

} // namespace SpacetimeDb

namespace SpacetimeDb::bsatn {

// Explicit specialization for ScheduleAt to handle BSATN serialization
template<>
struct bsatn_traits<::SpacetimeDb::ScheduleAt> {
    static void serialize(Writer& writer, const ::SpacetimeDb::ScheduleAt& value) {
        value.bsatn_serialize(writer);
    }
    
    static ::SpacetimeDb::ScheduleAt deserialize(Reader& reader) {
        ::SpacetimeDb::ScheduleAt result; // Default constructor
        result.bsatn_deserialize(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        // ScheduleAt is a special Sum type with Interval and Time variants
        // Use the existing TimeDuration and Timestamp algebraic_type specializations
        // to ensure proper inlining like the legacy path
        
        std::vector<SumTypeVariant> variants;
        
        // Interval variant: Use TimeDuration's algebraic_type() for proper inlining
        AlgebraicType interval_type = bsatn_traits<::SpacetimeDb::TimeDuration>::algebraic_type();
        variants.emplace_back("Interval", std::move(interval_type));
        
        // Time variant: Use Timestamp's algebraic_type() for proper inlining
        AlgebraicType time_type = bsatn_traits<::SpacetimeDb::Timestamp>::algebraic_type();
        variants.emplace_back("Time", std::move(time_type));
        
        auto sum_type = std::make_unique<SumTypeSchema>(std::move(variants));
        return AlgebraicType::make_sum(std::move(sum_type));
    }
};

} // namespace SpacetimeDb::bsatn

// BSATN serializer template specialization for ScheduleAt
namespace SpacetimeDb {

// ScheduleAt serialization - sum type with variant tag + payload
template<>
struct BsatnSerializer<ScheduleAt> {
    static void serialize(std::vector<uint8_t>& buffer, const ScheduleAt& value) {
        // Write variant tag
        buffer.push_back(static_cast<uint8_t>(value.get_variant()));
        
        // Write payload based on variant
        switch (value.get_variant()) {
            case ScheduleAt::Variant::Interval:
                BsatnSerializer<TimeDuration>::serialize(buffer, value.get_interval());
                break;
            case ScheduleAt::Variant::Time:
                BsatnSerializer<Timestamp>::serialize(buffer, value.get_time());
                break;
        }
    }
    
    static ScheduleAt deserialize(const uint8_t* data, size_t& offset) {
        // Read variant tag
        uint8_t variant_tag = data[offset++];
        
        switch (variant_tag) {
            case 0: { // Interval
                TimeDuration dur = BsatnSerializer<TimeDuration>::deserialize(data, offset);
                return ScheduleAt::interval(dur);
            }
            case 1: { // Time
                Timestamp ts = BsatnSerializer<Timestamp>::deserialize(data, offset);
                return ScheduleAt::time(ts);
            }
            default:
                return ScheduleAt::interval(TimeDuration::from_seconds(0));
        }
    }
};

} // namespace SpacetimeDb

#endif // SPACETIMEDB_BSATN_SCHEDULE_AT_IMPL_H