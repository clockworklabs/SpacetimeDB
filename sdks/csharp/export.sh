#!/bin/bash

if [ -z "$UNITY_PATH" ]; then
	unameOut="$(uname -s)"
	case "${unameOut}" in
	    Linux*)     export UNITY_PATH="$HOME/Unity/Hub/Editor/2020.3.25f1/Editor/Unity";;
	    Darwin*)    export UNITY_PATH="/Applications/Unity/Hub/Editor/2020.3.25f1/Unity.app/Contents/MacOS/Unity";;
	    CYGWIN*)    echo "Windows not supported, use WSL instead." && exit 1;;
	    MINGW*)     echo "Windows not supported, use WSL instead." && exit 1;;
	    *)          machine="UNKNOWN:${unameOut}"
	esac
	echo ${machine}

fi
	
if [ ! -f $UNITY_PATH ]; then
	echo "Unable to find a version of Unity. Please specify it using UNITY_PATH :)"	
	exit 1
fi

$UNITY_PATH -projectPath . -quit -batchmode -nographics -ignorecompilererrors -executeMethod "ExportPackage.Export"
