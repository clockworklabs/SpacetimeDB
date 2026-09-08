#nullable enable
namespace SpacetimeDB.Internal;

[SpacetimeDB.Type]
[System.Runtime.Serialization.DataContract]
public sealed partial class EnvironmentDeclaration
{
    [System.Runtime.Serialization.DataMember(Name = "name")]
    public string Name;

    [System.Runtime.Serialization.DataMember(Name = "constraint")]
    public EnvironmentConstraint Constraint;

    [System.Runtime.Serialization.DataMember(Name = "optional")]
    public bool Optional;

    public EnvironmentDeclaration(string Name, EnvironmentConstraint Constraint, bool Optional)
    {
        this.Name = Name;
        this.Constraint = Constraint;
        this.Optional = Optional;
    }

    public EnvironmentDeclaration()
    {
        Name = "";
        Constraint = new EnvironmentConstraint.AnyString(default);
    }
}
