#!/usr/bin/sh
(
    printf "\nBeginning deployment of minimal binary to AUR\n\n"
    cd ./auditorium-minimal
    ./deploy.sh
)
(
    printf "\nBeginning deployment of full binary to AUR\n\n"
    cd ./auditorium-full
    ./deploy.sh
)
