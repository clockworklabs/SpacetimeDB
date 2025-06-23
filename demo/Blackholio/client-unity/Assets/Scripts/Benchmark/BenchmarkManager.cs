using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

using System;
using System.Threading;
using System.Diagnostics;
using System.Linq;
using SpacetimeDB.Types;
using System.IO;
using SpacetimeDB.BSATN;

public class BenchmarkVisualizer : MonoBehaviour
{
    /// <summary>
    /// Class to prevent benchmark operations from being optimized out.
    /// </summary>
    class BlackBox
    {
        private static volatile int junkNum;
        private static volatile object junkObject;

        public static void Consume(double val)
        {
            junkNum = (int)val;
        }

        public static void Consume(float val)
        {
            junkNum = (int)val;
        }

        public static void Consume(long val)
        {
            junkNum = (int)val;
        }

        public static void Consume(byte val)
        {
            junkNum = val;
        }

        public static void Consume(int val)
        {
            junkNum = val;
        }

        public static void Consume<T>(T val)
        where T : class
        {
            junkObject = val;
        }

    }

    [Header("Benchmark Settings")]
    public int startN = 0;
    public int stepN = 10;
    public int count = 100;
    public int countPer = 5;

    [Header("Visualization Settings")]
    public double cubeSize = 0.5f;
    public Material cubeMaterial;
    public GameObject labelPrefab;

    List<GameObject> plotObjects = new();

    private List<(int N, double timeNs)> results = new List<(int, double)>();
    private double maxTime;
    private double mapY(double timeNs)
    {
        UnityEngine.Debug.Log($"timeNs: {timeNs}, maxTime: {maxTime}, result: {(timeNs / maxTime) * 100}");
        return (timeNs / maxTime) * 100;
    }
    private bool benchmarksComplete = false;

    void Start()
    {
    }

    void Update()
    {
        if (benchmarksComplete)
        {
            benchmarksComplete = false;
            ClearPlot();
            DrawResults();
        }
    }

    public static double MapRange(double inLo, double inVal, double inHi, double outLo, double outHi)
    {
        return (inVal - inLo) / (inHi - inLo) * (outHi - outLo) + outLo;
    }

    public void RunBenchmarks(string name)
    {
        results.Clear();
        var bench = benchmarks[name];
        // warmup
        for (int i = 0; i < count; i++)
        {
            int N = startN + i * stepN;

            for (int j = 0; j < countPer; j++)
            {
                bench(N);
            }
        }
        // execute
        for (int i = 0; i < count; i++)
        {
            int N = startN + i * stepN;

            for (int j = 0; j < countPer; j++)
            {
                var elapsed = bench(N);

                double timeNs = (double)elapsed.Ticks * 100.0;
                results.Add((N, timeNs));
            }
        }

        maxTime = SelectMaxWithoutOutliers(results.Select(v => v.timeNs));
        UnityEngine.Debug.Log(maxTime);

        benchmarksComplete = true;
    }


    public static List<double> RemoveHighOutliers(List<double> data)
    {
        if (data == null || data.Count < 4)
            return new List<double>(data); // Too few data points for IQR

        var sorted = data.OrderBy(x => x).ToList();

        double Q1 = GetPercentile(sorted, 25);
        double Q3 = GetPercentile(sorted, 75);
        double IQR = Q3 - Q1;
        double upperBound = Q3 + 1.5 * IQR;

        UnityEngine.Debug.Log($"upperBound: {upperBound}");

        return data.Where(x => x <= upperBound).ToList();
    }

    private static double GetPercentile(List<double> sortedData, double percentile)
    {
        if (sortedData == null || sortedData.Count == 0)
            throw new ArgumentException("Data list is empty");

        double position = (sortedData.Count + 1) * percentile / 100.0;
        int leftIndex = (int)Math.Floor(position) - 1;
        double fraction = position - Math.Floor(position);

        if (leftIndex < 0)
            return sortedData[0];
        if (leftIndex >= sortedData.Count - 1)
            return sortedData.Last();

        return sortedData[leftIndex] + fraction * (sortedData[leftIndex + 1] - sortedData[leftIndex]);
    }

    double SelectMaxWithoutOutliers(IEnumerable<double> numbersEnumerable, double thresholdStdDevs = 2.0)
    {
        List<double> numbers = numbersEnumerable.ToList();

        // Filter values within 2 standard deviations and get the max
        return RemoveHighOutliers(numbers)
            .Max();
    }

    Dictionary<string, Func<int, TimeSpan>> benchmarks = new()
    {
        { "Serialize", SerializeBench },
        { "Apply", ApplyBench }
    };

    static Dictionary<int, SerializeTestCase<BucketOfVectors>> PrefabbedSerializeData = new();
    struct SerializeTestCase<T>
    {
        public MemoryStream Serialized;
        public IReadWrite<T> Serializer;
    }
    static TimeSpan SerializeBench(int N)
    {
        // Prepare computation (only runs during setup)
        if (!PrefabbedSerializeData.TryGetValue(N, out var testCase))
        {
            var random = new System.Random();
            var prefabUnser = new BucketOfVectors { TheVectors = Enumerable.Range(0, N).Select(_ => new DbVector2((float)random.NextDouble(), (float)random.NextDouble())).ToList() };
            var stream = new MemoryStream();
            var writer = new BinaryWriter(stream);
            var serializer = (IReadWrite<BucketOfVectors>)((IStructuralReadWrite)prefabUnser).GetSerializer();
            serializer.Write(writer, prefabUnser);
            testCase = new SerializeTestCase<BucketOfVectors> { Serialized = stream, Serializer = serializer };
            PrefabbedSerializeData[N] = testCase;
        }

        Stopwatch stopwatch = new Stopwatch();
        stopwatch.Start();

        // Simulate computation
        testCase.Serialized.Seek(0, SeekOrigin.Begin);
        var reader = new BinaryReader(testCase.Serialized);
        BlackBox.Consume(testCase.Serializer.Read(reader));

        stopwatch.Stop();
        return stopwatch.Elapsed;
    }


    struct ApplyTestCase
    {
        // TODO: need to make MultiDictionary and MultiDictionaryDelta visible to this project.
    }
    static TimeSpan ApplyBench(int N)
    {
        Stopwatch stopwatch = new Stopwatch();
        stopwatch.Start();
        for (int i = 0; i < N * 1000; i++)
        {
            BlackBox.Consume(i);
        } 
        // ...
        stopwatch.Stop();
        return stopwatch.Elapsed;
    }

    void DrawResults()
    {
        var maxN = startN + stepN * count;
        foreach (var (N, time) in results)
        {
            Vector3 pos = new Vector3((float)MapRange(0, N, maxN, 0, 100), (float)MapRange(0, time, maxTime, 0, 100), 0);
            GameObject cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
            cube.transform.position = pos;
            cube.transform.localScale = Vector3.one * (float)cubeSize;
            if (cubeMaterial != null)
                cube.GetComponent<Renderer>().material = cubeMaterial;
            plotObjects.Add(cube);
        }

        DrawAxisLabels();
    }

    void DrawAxisLabels()
    {
        if (labelPrefab == null) return;

        HashSet<int> seen = new();

        var maxN = startN + stepN * count;

        for (int i = 0; i < 20; i++)
        {
            GameObject xLabel = Instantiate(labelPrefab, new Vector3((float)MapRange(0, i, 20, 0, 100), -1, 0), Quaternion.identity);
            xLabel.GetComponent<TextMesh>().text = $"{MapRange(0, i, 20, 0, maxN)}";
            plotObjects.Add(xLabel);
        }
        for (int i = 0; i < 20; i++)
        {
            GameObject yLabel = Instantiate(labelPrefab, new Vector3(-1, (float)MapRange(0, i, 20, 0, 100), 0), Quaternion.identity);
            yLabel.GetComponent<TextMesh>().text = $"{MapRange(0, i, 20, 0, maxTime)}ns";
            plotObjects.Add(yLabel);
        }

    }
    void ClearPlot()
    {
        foreach (var obj in plotObjects) {
            DestroyImmediate(obj);
        }
        plotObjects.Clear();
    }
}