import { SpacetimeDBClient, ProductValue, AlgebraicValue, AlgebraicType, BuiltinTypeType, ProductTypeElement, SumType, SumTypeVariant } from "./spacetimedb";

let token = process.env.STDB_TOKEN;
let identity = process.env.STDB_IDENTITY;
let db_name = process.env.STDB_DATABASE;

// let type = AlgebraicType.createProductType([
//   new ProductTypeElement("name", AlgebraicType.createPrimitiveType(BuiltinTypeType.String))
// ]);
// let product = AlgebraicValue.deserialize(type, ["foo"]);

class Address {
  public street: string;
  public zipcode: string;
  public country: string;

  constructor(street: string, zipcode: string, country: string) {
    this.street = street;
    this.zipcode = zipcode;
    this.country = country;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("street", AlgebraicType.createPrimitiveType(BuiltinTypeType.String)),
      new ProductTypeElement("zipcode", AlgebraicType.createPrimitiveType(BuiltinTypeType.String)),
      new ProductTypeElement("country", AlgebraicType.createPrimitiveType(BuiltinTypeType.String))
    ]);
  }

  public static fromValue(value: AlgebraicValue): Address {
    let productValue = value.asProductValue();
		let street = productValue.elements[0].asString();
		let zipcode = productValue.elements[1].asString();
		let country = productValue.elements[2].asString();

    return new Address(street, zipcode, country);
  }
}

class Education {
  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createSumType([
      new SumTypeVariant("None", AlgebraicType.createPrimitiveType(BuiltinTypeType.String)),
      new SumTypeVariant("High", AlgebraicType.createPrimitiveType(BuiltinTypeType.String)),
      new SumTypeVariant("Higher", AlgebraicType.createPrimitiveType(BuiltinTypeType.String))
    ]);
  }

  public static fromValue(value: AlgebraicValue): Education {
    return new Education();
  }
}

class Person {
  public name: string;
  public education: Education;
  public address: Address;

  constructor(name: string, education: Education, address: Address) {
    this.name = name;
    this.education = education;
    this.address = address;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("name", AlgebraicType.createPrimitiveType(BuiltinTypeType.String)),
      new ProductTypeElement("education", Education.getAlgebraicType()),
      new ProductTypeElement("address", Address.getAlgebraicType()),
    ])
  }

  public static fromValue(value: AlgebraicValue): Person {
    let productValue = value.asProductValue();
		let name: string = productValue.elements[0].asString();
		let education: Education = Education.fromValue(productValue.elements[1]);
		let address: Address = Address.fromValue(productValue.elements[2]);

    return new Person(name, education, address);
  }
}

let object = ["foo2",{"0":[]},["Frankfurter Allee 53","10247","Germany"]];
let v = AlgebraicValue.deserialize(Person.getAlgebraicType(), object);
console.log(v);
console.log(Person.fromValue(v));
// let client = new SpacetimeDBClient("localhost:3000", db_name, {identity: identity, token: token});
