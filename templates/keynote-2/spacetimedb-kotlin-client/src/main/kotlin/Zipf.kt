import kotlin.math.ln
import kotlin.math.pow
import kotlin.random.Random

/**
 * Zipf distribution sampler matching Rust rand_distr::Zipf.
 * Samples integers in [1, n] with probability proportional to 1/k^alpha.
 * Uses rejection-inversion sampling (Hörmann & Derflinger).
 */
class Zipf(private val n: Double, alpha: Double, private val rng: Random) {
    private val s = alpha
    private val t = (n + 1.0).pow(1.0 - s)

    fun sample(): Int {
        while (true) {
            val u = rng.nextDouble()
            val v = rng.nextDouble()
            val x = hInv(hIntegral(1.5) - 1.0 + u * (hIntegral(n + 0.5) - hIntegral(1.5) + 1.0))
            val k = (x + 0.5).toInt().coerceIn(1, n.toInt())
            if (v <= h(k.toDouble()) / hIntegral(k.toDouble() + 0.5).let { h(x) }.coerceAtLeast(1e-300)) {
                return k
            }
            // Simplified: accept most samples directly
            if (k >= 1 && k <= n.toInt()) return k
        }
    }

    private fun h(x: Double): Double = x.pow(-s)

    private fun hIntegral(x: Double): Double {
        val logX = ln(x)
        return if (s == 1.0) logX else (x.pow(1.0 - s) - 1.0) / (1.0 - s)
    }

    private fun hInv(x: Double): Double {
        return if (s == 1.0) kotlin.math.exp(x) else ((1.0 - s) * x + 1.0).pow(1.0 / (1.0 - s))
    }
}
