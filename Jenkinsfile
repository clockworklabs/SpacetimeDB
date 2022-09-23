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

    stage('Build SpacetimeDB Image') {
      def spacetimedb = docker.build("clockwork/spacetimedb", ". -f crates/spacetimedb/Dockerfile")
      docker.withRegistry('https://registry.digitalocean.com') {
        spacetimedb.push()
        SPACETIMEDB_IMAGE_DIGEST = imageDigest("clockwork/spacetimedb")
        spacetimedb.push("${WEBSITE_IMAGE_DIGEST}")
      }
    }

    stage('Deploy') {
      if (env.BUILD_ENV == "testing") {
        withCredentials([
          string(credentialsId: 'TESTING_KUBERNETES_CLUSTER_CERTIFICATE', variable: 'KUBERNETES_CLUSTER_CERTIFICATE'),
          string(credentialsId: 'TESTING_KUBERNETES_SERVER', variable: 'KUBERNETES_SERVER'),
          string(credentialsId: 'TESTING_KUBERNETES_TOKEN', variable: 'KUBERNETES_TOKEN')]) {
          sh "export BUILD_ENV=${env.BUILD_ENV}\
              WEBSITE_IMAGE_DIGEST=$WEBSITE_IMAGE_DIGEST\
              && ./kubernetes-deploy.sh"
          }
      } else if(env.BUILD_ENV == "live" || env.BUILD_ENV == "staging") {
        withCredentials([
          string(credentialsId: 'KUBERNETES_CLUSTER_CERTIFICATE', variable: 'KUBERNETES_CLUSTER_CERTIFICATE'),
          string(credentialsId: 'KUBERNETES_SERVER', variable: 'KUBERNETES_SERVER'),
          string(credentialsId: 'KUBERNETES_TOKEN', variable: 'KUBERNETES_TOKEN')]) {
          sh "export BUILD_ENV=${env.BUILD_ENV}\
              WEBSITE_IMAGE_DIGEST=$WEBSITE_IMAGE_DIGEST\
              && ./kubernetes-deploy.sh"
          }
      }
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
