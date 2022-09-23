import org.jenkinsci.plugins.workflow.steps.FlowInterruptedException

String imageDigest(name) {
  sh(returnStdout: true, script: "docker inspect --format='{{index .RepoDigests 0}}' $name | cut -d':' -f 2").trim()
}

node {
  properties([
    disableConcurrentBuilds(),
    githubProjectProperty(displayName: 'SpacetimeDB', projectUrlStr: 'https://github.com/clockworklabs/SpacetimeDB/'),
  ])

  try {
    def GRAFANA_IMAGE_DIGEST
    def PROMETHEUS_IMAGE_DIGEST 
    def SPACETIMEDB_IMAGE_DIGEST 
    stage('Clone Repository') {
      checkout scm
    }

    stage('Build Grafana Image') {
      def grafana = docker.build("clockwork/spacetimedb_grafana", "packages/grafana")
      docker.withRegistry('https://registry.digitalocean.com') {
        grafana.push()
        GRAFANA_IMAGE_DIGEST = imageDigest("clockwork/spacetimedb_grafana")
        grafana.push("${GRAFANA_IMAGE_DIGEST}")
      }
    }

    stage('Build Prometheus Image') {
      def prometheus = docker.build("clockwork/spacetimedb_prometheus", "packages/prometheus")
      docker.withRegistry('https://registry.digitalocean.com') {
        prometheus.push()
        PROMETHEUS_IMAGE_DIGEST = imageDigest("clockwork/spacetimedb_prometheus")
        prometheus.push("${PROMETHEUS_IMAGE_DIGEST}")
      }
    }

    stage('Build SpacetimeDB Image') j{
      def spacetimedb = docker.build("clockwork/spacetimedb", "crates -f crates/spacetimedb/Dockerfile")
      docker.withRegistry('https://registry.digitalocean.com') {
        spacetimedb.push()
        SPACETIMEDB_IMAGE_DIGEST = imageDigest("clockwork/spacetimedb")
        spacetimedb.push("${WEBSITE_IMAGE_DIGEST}")
      }
    }

    stage('Deploy') {
    }
  } catch (FlowInterruptedException interruptEx) {
    currentBuild.result = "ABORTED"
    throw interruptEx;
  } catch (err) {
    currentBuild.result = "FAILURE"
    throw err;
  } finally {
    // print discord message
    // discordSend description: "", link: env.BUILD_URL, result: currentBuild.currentResult, title: "", webhookURL: ""
  }
}
