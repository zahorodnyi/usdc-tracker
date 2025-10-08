FROM ubuntu:latest
LABEL authors="oleksandrzagorodnij"

ENTRYPOINT ["top", "-b"]