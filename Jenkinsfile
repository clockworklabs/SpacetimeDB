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
      docker.withRegistry('https://registry.digitalocean.com', 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS') {
        grafana.push()
        GRAFANA_IMAGE_DIGEST = imageDigest("clockwork/spacetimedb_grafana")
        grafana.push("${GRAFANA_IMAGE_DIGEST}")
      }
    }

    stage('Build Prometheus Image') {
      def prometheus = docker.build("clockwork/spacetimedb_prometheus", "packages/prometheus")
      docker.withRegistry('https://registry.digitalocean.com', 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS') {
        prometheus.push()
        PROMETHEUS_IMAGE_DIGEST = imageDigest("clockwork/spacetimedb_prometheus")
        prometheus.push("${PROMETHEUS_IMAGE_DIGEST}")
      }
    }

    stage('Build SpacetimeDB Image') {
      def spacetimedb = docker.build("clockwork/spacetimedb", ". -f crates/spacetimedb-core/Dockerfile")
      docker.withRegistry('https://registry.digitalocean.com', 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS') {
        spacetimedb.push()
        SPACETIMEDB_IMAGE_DIGEST = imageDigest("clockwork/spacetimedb")
        spacetimedb.push("${SPACETIMEDB_IMAGE_DIGEST}")
      }
    }

    stage('Deploy') {
      if (env.BUILD_ENV == "testing") {
        withCredentials([usernamePassword(credentialsId: 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS', usernameVariable: 'USERNAME', passwordVariable: 'PASSWORD'),]) {
          withCredentials([sshUserPrivateKey(credentialsId: "AWS_EC2_INSTANCE_JENKINS_SSH_KEY", keyFileVariable: 'keyfile')]) {
            sh "scp -o StrictHostKeyChecking=accept-new -P 9001 -i '${keyfile}' docker-compose-live.yml jenkins@vpn.partner.spacetimedb.net:/home/jenkins/docker-compose-live.yml"
            sh "ssh -o StrictHostKeyChecking=accept-new -p 9001 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net 'docker login -u ${USERNAME} -p ${PASSWORD} https://registry.digitalocean.com; docker-compose -f docker-compose-live.yml stop; docker-compose -f docker-compose-live.yml pull; docker-compose -f docker-compose-live.yml up -d'"
          }
        }
      } else if (env.BUILD_ENV == "live") {
        withCredentials([usernamePassword(credentialsId: 'DIGITAL_OCEAN_DOCKER_REGISTRY_CREDENTIALS', usernameVariable: 'USERNAME', passwordVariable: 'PASSWORD'),]) {
          withCredentials([sshUserPrivateKey(credentialsId: "AWS_EC2_INSTANCE_JENKINS_SSH_KEY", keyFileVariable: 'keyfile')]) {
            sh "scp -o StrictHostKeyChecking=accept-new -P 9000 -i '${keyfile}' docker-compose-live.yml jenkins@vpn.partner.spacetimedb.net:/home/jenkins/docker-compose-live.yml"
            sh "ssh -o StrictHostKeyChecking=accept-new -p 9000 -i '${keyfile}' jenkins@vpn.partner.spacetimedb.net 'docker login -u ${USERNAME} -p ${PASSWORD} https://registry.digitalocean.com; docker-compose -f docker-compose-live.yml stop; docker-compose -f docker-compose-live.yml pull; docker-compose -f docker-compose-live.yml up -d'"
          }
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
