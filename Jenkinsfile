pipeline {
  agent any

  stages {

    stage('Git Checkout') {
      steps {
        checkout scm
      }
    }

    stage('Public-Private Tests') {
      steps {
        build job: "SpacetimeDB Private-Public Compatibility",
	parameters: [
	  string(
	    name: 'SPACETIMEDB_PUBLIC_BRANCH',
	    value: 'master'
	  )
	]
      }
    }

  }
}