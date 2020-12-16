let io;
document.addEventListener("DOMContentLoaded", function() {
    // connect to ws
    io = new WebSocket( `${window.location.protocol === "https:" ? "wss" : "ws"}://${window.location.host}${window.location.pathname}` );

    io.addEventListener('message', message => {
        console.log('Message from server', message.data);
        const data = JSON.parse(message.data);
        switch( data.cmd ) {
            case 'STATE':
                gameState = data.play_book;
                // redraw
                redrawPlayBook();
                if( ! handleResultValidation() ) handlePlayerChange();
                break;
            case 'INIT':
                console.log( 'init' );
                localPlayer = data.player == "X" ? "X" : "O";
                gameState = data.play_book;
                // redraw
                redrawPlayBook();
                handlePlayerChange();
                gameActive = localPlayer === currentPlayer ? true : false;
                break;
            case 'RESET':
                console.log( 'reset' );
                if( ! reset_by_me ) alert( 'Other player just RESET the game' );

                gameState = data.play_book;
                // redraw
                redrawPlayBook();
                handlePlayerChange();
                gameActive = localPlayer === currentPlayer ? true : false;
                reset_by_me = false;
                break;
            case 'COMPLETE':
                console.log( 'COMPLETE' );
                alert( 'Oops... board complete!' );
                statusDisplay.innerHTML = COMPLETE_MSG;
                statusDisplay.classList.add("complete");
                document.querySelector('.game--restart').classList.add("hidden");
                gameActive = false;
                break;
            case 'LEAVE':
                console.log( 'leave' );
                alert( 'Other player just LEAVE the game' );
                break;
        };
    });
});

//
const winningMessage = () => `Player <span class="tide">${currentPlayer}</span> has won!`;
const drawMessage = () => `Game ended in a draw!`;
const currentPlayerTurn = () => `It's <span class="tide">${currentPlayer}</span>'s turn. <span class="game--player">(You are ${localPlayer})</span>`;
const COMPLETE_MSG = `Board complete! please <a href="/">home</a> and create or join another board`

const winningConditions = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8],
    [0, 3, 6],
    [1, 4, 7],
    [2, 5, 8],
    [0, 4, 8],
    [2, 4, 6]
];



let gameActive = false;
let currentPlayer = "";
let localPlayer = "";
let gameState = ["", "", "", "", "", "", "", "", ""];
let reset_by_me = false;

function init() {
    io.send('play');
}


const statusDisplay = document.querySelector('.game--status');
statusDisplay.innerHTML = `Initializing...`;

function handleCellPlayedLocally(clickedCell, clickedCellIndex) {
    gameState[clickedCellIndex] = currentPlayer;
    //clickedCell.innerHTML = currentPlayer;
    clickedCell.classList.add(currentPlayer.toLocaleLowerCase());
    io.send( `PLAY:${localPlayer}:${clickedCellIndex }` );
    gameActive = false;
}

function redrawPlayBook() {
    document.querySelectorAll('.cell').forEach( cell => {
        const play = gameState[ cell.dataset.cellIndex ];
        if( play !== "" ) cell.classList.add(play.toLocaleLowerCase());
        else cell.classList.remove( "x", "o", "win" );
    });
}

function handlePlayerChange() {
    const plays = gameState.filter( x => x !== '' );
    currentPlayer = plays.length % 2 === 0 ? "X" : "O";
    statusDisplay.innerHTML = currentPlayerTurn();

    gameActive = currentPlayer === localPlayer;
}

function handleResultValidation() {
    let roundWon = false;
    let winCondition;
    for (let i = 0; i <= 7; i++) {
        winCondition = winningConditions[i];
        let a = gameState[winCondition[0]];
        let b = gameState[winCondition[1]];
        let c = gameState[winCondition[2]];
        if (a === '' || b === '' || c === '') {
            continue;
        }
        if (a === b && b === c) {
            roundWon = true;
            break
        }
    }

    if (roundWon) {
        cells = document.querySelectorAll( '.cell' );
        winCondition.forEach( index => {
            var cell = cells[ index ];
            cell.classList.add("win");
        });

        statusDisplay.innerHTML = winningMessage();
        gameActive = false;
        return true;
    }

    let roundDraw = !gameState.includes("");
    if (roundDraw) {
        statusDisplay.innerHTML = drawMessage();
        gameActive = false;
        return true;
    }
}

function handleCellClick(clickedCellEvent) {
    const clickedCell = clickedCellEvent.target;
    const clickedCellIndex = parseInt(clickedCell.getAttribute('data-cell-index'));

    if (gameState[clickedCellIndex] !== "" || !gameActive) {
        return;
    }

    handleCellPlayedLocally(clickedCell, clickedCellIndex);
    handleResultValidation();
}

function handleRestartGame() {
    reset_by_me = true;
    io.send( `RESET:${localPlayer}` );
}


// init client
document.querySelectorAll('.cell').forEach(cell => cell.addEventListener('click', handleCellClick));
document.querySelector('.game--restart').addEventListener('click', handleRestartGame);

window.addEventListener('beforeunload', function(event) {
    io.send(`LEAVE:${localPlayer}`);
});
