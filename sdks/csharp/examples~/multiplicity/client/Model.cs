using SpacetimeDB.Types;

namespace client;

class Model
{
    public HashSet<Dog> ExpectedServerDogs = new HashSet<Dog>();
    public HashSet<Cat> ExpectedServerCats = new HashSet<Cat>();
    
    public HashSet<Dog> ExpectedClientDogs = new HashSet<Dog>();
    public HashSet<Cat> ExpectedClientCats = new HashSet<Cat>();

    public void AddDog(Dog dog)
    {
        ExpectedServerDogs.Add(dog);
    }
    
    public void UpdateDog(Dog dog)
    {
        if (!ContainsDog(dog.Name))
        {
            Console.ForegroundColor = ConsoleColor.Red;
            Console.WriteLine($"No Dog with Name:{dog.Name} found in local model. Cannot update local model.");
            Console.ForegroundColor = ConsoleColor.White;
            return;
        }
        foreach (Dog existingDog in ExpectedServerDogs)
        {
            if (existingDog.Name == dog.Name)
            {
                existingDog.Name = dog.Name;
                existingDog.Color = dog.Color;
                existingDog.Age = dog.Age;
            }
        }
    }

    public void RemoveDog(Dog dog)
    {
        ExpectedServerDogs.Remove(dog);
    }
    
    public void RemoveDog(string name)
    {
        foreach (Dog dog in ExpectedServerDogs)
        {
            if (dog.Name == name)
            {
                ExpectedServerDogs.Remove(dog);
            }
        }
    }
    
    public bool ContainsDog(string name)
    {
        bool contains = false;
        foreach (Dog dog in ExpectedServerDogs)
        {
            if (dog.Name == name)
            {
                contains = true;
            }
        }
        return contains;
    }
    
    public bool ContainsDog(string name, string color, uint age, HashSet<Dog> modelHashSet)
    {
        bool contains = false;
        foreach (Dog dog in modelHashSet)
        {
            if (dog.Name == name && dog.Color == color && dog.Age == age)
            {
                contains = true;
            }
        }
        return contains;
    }

    public void AddCat(Cat cat)
    {
        ExpectedServerCats.Add(cat);
    }
    
    public void UpdateCat(Cat cat)
    {
        if (!ContainsDog(cat.Name))
        {
            Console.ForegroundColor = ConsoleColor.Red;
            Console.WriteLine($"No Dog with Name:{cat.Name} found in local model. Cannot update local model.");
            Console.ForegroundColor = ConsoleColor.White;
            return;
        }
        foreach (Dog existingCat in ExpectedServerDogs)
        {
            if (existingCat.Name == cat.Name)
            {
                existingCat.Name = cat.Name;
                existingCat.Color = cat.Color;
                existingCat.Age = cat.Age;
            }
        }
    }

    public void RemoveCat(Cat cat)
    {
        ExpectedServerCats.Remove(cat);
    }
    
    public void RemoveCat(string name)
    {
        foreach (Cat cat in ExpectedServerCats)
        {
            if (cat.Name == name)
            {
                ExpectedServerCats.Remove(cat);
            }
        }
    }
    
    public bool ContainsCat(string name)
    {
        bool contains = false;
        foreach (Cat cat in ExpectedServerCats)
        {
            if (cat.Name == name)
            {
                contains = true;
            }
        }
        return contains;
    }
    
    public bool ContainsCat(string name, string color, uint age, HashSet<Cat> modelHashSet)
    {
        bool contains = false;
        foreach (Cat cat in modelHashSet)
        {
            if (cat.Name == name && cat.Color == color && cat.Age == age)
            {
                contains = true;
            }
        }
        return contains;
    }

    public void OutputExpectedDogs(HashSet<Dog> modelHashSet)
    {
        Console.WriteLine("Client dogs:");
        foreach (Dog dog in modelHashSet)
        {
            Console.WriteLine($"  Dog (Name:{dog.Name}, Color:{dog.Color}, Age:{dog.Age}).");
        }
    }
    
    public void OutputExpectedCats(HashSet<Cat> modelHashSet)
    {
        Console.WriteLine("Client dogs:");
        foreach (Cat cat in modelHashSet)
        {
            Console.WriteLine($"  Cat (Name:{cat.Name}, Color:{cat.Color}, Age:{cat.Age}).");
        }
    }
}